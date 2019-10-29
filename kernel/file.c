
#include <ntifs.h>
#include "globals.h"
#include "log.h"
#include "utils.h"
#include "file.h"

PVOLUME_INFO gVolumeNamesList = NULL;
PPAGING_FILEINFO_LIST gPagingFileInfoList = NULL;

PUNICODE_STRING
FindPagingFileNameInList(
	_In_ PFILE_OBJECT FileObject
)
/*++

Routine Description:

	.

Arguments:

	FileObject -

Return Value:

	Routine can return non success error codes.

--*/
{
	KIRQL OldIrql = DISPATCH_LEVEL;
	PUNICODE_STRING pStrNameInfo = NULL;
	PPAGING_FILEINFO_LIST pFileNameInfo;

	if (KeGetCurrentIrql() < DISPATCH_LEVEL)
		OldIrql = KeAcquireSpinLockRaiseToDpc(&gFileNameInfoListSpinLock);
	if (gPagingFileInfoList) {

		for (pFileNameInfo = gPagingFileInfoList;
			pFileNameInfo;
			pFileNameInfo = pFileNameInfo->Next)
		{

			//
			// If the fileobject match we pass in
			// then copy the filename
			//

			if (pFileNameInfo->FileObject == FileObject) {
				pStrNameInfo = ProcmonDuplicateUnicodeString(NonPagedPool,
					&pFileNameInfo->FileName, '1');
			}
		}
	}
	if (OldIrql != DISPATCH_LEVEL)
		KeReleaseSpinLock(&gFileNameInfoListSpinLock, OldIrql);
	return pStrNameInfo;
}

VOID
AddToPagingFileNameList(
	_In_ PFILE_OBJECT FileObject,
	_In_ PUNICODE_STRING pStrFileName
)
/*++

Routine Description:

	.

Arguments:

	FileObject -
	pStrFileName -

Return Value:

	None.

--*/
{
	KIRQL OldIrql;
	PPAGING_FILEINFO_LIST pNewInfo;

	OldIrql = KeAcquireSpinLockRaiseToDpc(&gFileNameInfoListSpinLock);

	//
	// Check is the fileobject already in list
	//

	if (!FindPagingFileNameInList(FileObject)) {
		pNewInfo = (PPAGING_FILEINFO_LIST)ProcmonAllocatePoolWithTag(NonPagedPool,
			pStrFileName->Length + sizeof(PAGING_FILEINFO_LIST), '1');
		if (pNewInfo) {

			//
			// Fill the info
			//

			pNewInfo->FileObject = FileObject;
			pNewInfo->FileName.Length = pStrFileName->Length;
			pNewInfo->FileName.Buffer = (PWSTR)(pNewInfo + 1);
			RtlCopyMemory(pNewInfo->FileName.Buffer, pStrFileName->Buffer, pStrFileName->Length);

			//
			// Add to list
			//

			pNewInfo->Next = gPagingFileInfoList;
			gPagingFileInfoList = pNewInfo;
		}
	}
	KeReleaseSpinLock(&gFileNameInfoListSpinLock, OldIrql);
}


BOOLEAN
ProcmonIsFileInSystemRoot(
	_In_ PUNICODE_STRING pUniStrImageName
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	USHORT SavedLength;
	LONG nRet;
	BOOLEAN bRet = FALSE;

	SavedLength = pUniStrImageName->Length;
	if (pUniStrImageName->Length <= gUniStrSystemRoot.Length)
		return FALSE;
	pUniStrImageName->Length = gUniStrSystemRoot.Length;
	nRet = RtlCompareUnicodeString(&gUniStrSystemRoot, pUniStrImageName, TRUE);
	pUniStrImageName->Length = SavedLength;
	if (!nRet)
		bRet = 1;
	return bRet;
}

BOOLEAN
ProcmonIsFileExist(
	_In_ PUNICODE_STRING pUniStrFileName
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	IO_STATUS_BLOCK IoStatusBlock;
	OBJECT_ATTRIBUTES ObjectAttributes;
	HANDLE FileHandle;

	InitializeObjectAttributes(&ObjectAttributes, pUniStrFileName, 0x240, NULL, NULL);

	Status = FltCreateFile(gFilterHandle, NULL, &FileHandle, 0,
		&ObjectAttributes, &IoStatusBlock, NULL, 0, 7u, 1u, 0, NULL, 0, 0);
	if (NT_SUCCESS(Status))
		FltClose(FileHandle);
	return Status == STATUS_SUCCESS;
}

BOOLEAN
ProcmonAppendVolumeName(
	_In_ PCUNICODE_STRING pUniStrImageName,
	_Inout_ PUNICODE_STRING pUniStrFullName
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	ULONG Index, IndexTmp;
	BOOLEAN ErrorMode;
	VOLUME_INFO *pVolumeInfo;
	USHORT Length;

	Index = 0;
	ErrorMode = IoSetThreadHardErrorMode(FALSE);
	while (1)
	{
		ExAcquireFastMutex(&gMutexVolume);
		pVolumeInfo = gVolumeNamesList;
		if (Index)
		{
			IndexTmp = Index;
			do
			{
				pVolumeInfo = pVolumeInfo->Next;
				--IndexTmp;
			} while (IndexTmp);
		}
		if (!pVolumeInfo)
			break;
		Length = pUniStrImageName->Length + pVolumeInfo->Name.Length;
		pUniStrFullName->MaximumLength = Length;
		pUniStrFullName->Buffer = (PWCH)ProcmonAllocatePoolWithTag(NonPagedPool, Length, 'I');;
		if (!pUniStrFullName->Buffer)
			break;
		pUniStrFullName->Length = pVolumeInfo->Name.Length;
		RtlCopyMemory(pUniStrFullName->Buffer, pVolumeInfo->Name.Buffer, pVolumeInfo->Name.Length);
		ExReleaseFastMutex(&gMutexVolume);
		RtlAppendUnicodeStringToString(pUniStrFullName, pUniStrImageName);
		if (ProcmonIsFileExist(pUniStrFullName))
			return IoSetThreadHardErrorMode(ErrorMode);
		ExFreePoolWithTag(pUniStrFullName->Buffer, 0);
		pUniStrFullName->Buffer = NULL;
		++Index;
	}
	ExReleaseFastMutex(&gMutexVolume);
	return IoSetThreadHardErrorMode(ErrorMode);
}

VOID
ProcmonEnumAllVolumes(
	VOID
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	ULONG NumberVolumesReturned;
	PFLT_VOLUME *ppVolums;

	ExAcquireFastMutex(&gMutexVolume);

	//
	// Clear All volume name info
	//

	for (PVOLUME_INFO pVolumeInfo = gVolumeNamesList; gVolumeNamesList; pVolumeInfo = gVolumeNamesList)
	{
		gVolumeNamesList = pVolumeInfo->Next;
		ExFreePoolWithTag(pVolumeInfo->Name.Buffer, 0);
		ExFreePoolWithTag(pVolumeInfo, 0);
	}

	Status = FltEnumerateVolumes(gFilterHandle, NULL, 0, &NumberVolumesReturned);
	if (Status != STATUS_BUFFER_TOO_SMALL) {
		ExReleaseFastMutex(&gMutexVolume);
		return;
	}

	ppVolums = (PFLT_VOLUME *)ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(PFLT_VOLUME) * NumberVolumesReturned, 'b');
	if (!ppVolums) {
		ExReleaseFastMutex(&gMutexVolume);
		return;
	}

	Status = FltEnumerateVolumes(gFilterHandle, ppVolums, NumberVolumesReturned, &NumberVolumesReturned);
	if (NT_SUCCESS(Status)) {

		for (ULONG i = 0; i < NumberVolumesReturned; i++)
		{
			BOOLEAN IsDiskObject = FALSE;
			DEVICE_TYPE DeviceType = IO_TYPE_DEVICE;
			PDEVICE_OBJECT pVolumeDevObj;
			PFLT_VOLUME pVolumes = ppVolums[i];
			Status = FltGetDiskDeviceObject(pVolumes, &pVolumeDevObj);
			if (NT_SUCCESS(Status)) {
				if (pVolumeDevObj->DeviceType == IO_TYPE_MASTER_ADAPTER ||
					pVolumeDevObj->DeviceType == IO_TYPE_CONTROLLER) {
					IsDiskObject = TRUE;
					DeviceType = pVolumeDevObj->DeviceType;
				}
				ObDereferenceObject(pVolumeDevObj);
				if (IsDiskObject) {
					BOOLEAN bOk = FALSE;
					ULONG BufferSizeNeeded;
					PVOLUME_INFO pVolumeInfo = NULL;

					do
					{
						PVOLUME_INFO *ppVolumeNext;
						PVOLUME_INFO pVolumeInfoInsert;

						pVolumeInfo = (PVOLUME_INFO)ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(VOLUME_INFO), 'b');
						if (!pVolumeInfo) {
							break;
						}

						//
						// Initialize Volume Info
						//

						pVolumeInfo->Type = DeviceType;
						pVolumeInfo->Name.Buffer = NULL;
						Status = FltGetVolumeName(pVolumes, NULL, &BufferSizeNeeded);
						if (Status != STATUS_BUFFER_TOO_SMALL) {
							break;
						}

						pVolumeInfo->Name.Buffer = (PWCH)ProcmonAllocatePoolWithTag(NonPagedPool, BufferSizeNeeded, 'b');
						if (!pVolumeInfo->Name.Buffer){
							break;
						}

						pVolumeInfo->Name.MaximumLength = (USHORT)BufferSizeNeeded;
						Status = FltGetVolumeName(pVolumes, &pVolumeInfo->Name, NULL);
						if (!NT_SUCCESS(Status)) {
							break;
						}

						ppVolumeNext = &gVolumeNamesList;
						if (DeviceType == IO_TYPE_MASTER_ADAPTER) {
							for (pVolumeInfoInsert = gVolumeNamesList; pVolumeInfoInsert; pVolumeInfoInsert = pVolumeInfoInsert->Next) {
								if (pVolumeInfoInsert->Type != IO_TYPE_MASTER_ADAPTER) {
									break;
								}
								ppVolumeNext = &pVolumeInfoInsert->Next;
							}
						}
						else {
							for (pVolumeInfoInsert = gVolumeNamesList; pVolumeInfoInsert; pVolumeInfoInsert = pVolumeInfoInsert->Next) {
								if (pVolumeInfoInsert->Type == IO_TYPE_CONTROLLER) {
									break;
								}
								ppVolumeNext = &pVolumeInfoInsert->Next;
							}
						}

						pVolumeInfo->Next = *ppVolumeNext;
						*ppVolumeNext = pVolumeInfo;
						bOk = TRUE;
					} while (FALSE);

					if (!bOk) {
						if (pVolumeInfo) {
							if (pVolumeInfo->Name.Buffer) {
								ExFreePoolWithTag(pVolumeInfo->Name.Buffer, 0);
							}
							ExFreePoolWithTag(pVolumeInfo, 0);
						}
					}
				}
			}
			FltObjectDereference(pVolumes);
		}
	}

	ExFreePoolWithTag(ppVolums, 0);
	ExReleaseFastMutex(&gMutexVolume);
}


NTSTATUS
EnableFileMonitor(
	_In_ BOOLEAN bEnable
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	PFLT_VOLUME *ppVolumes;
	ULONG NumberVolumesReturned = 0;
	LARGE_INTEGER Interval;

	FltEnumerateVolumes(gFilterHandle, NULL, 0, &NumberVolumesReturned);
	if (!NumberVolumesReturned)
		return STATUS_SUCCESS;

	ppVolumes = ProcmonAllocatePoolWithTag(PagedPool, sizeof(PFLT_VOLUME) * NumberVolumesReturned, 'I');
	if (ppVolumes) {
		Status = FltEnumerateVolumes(gFilterHandle, ppVolumes, sizeof(PFLT_VOLUME) * NumberVolumesReturned,
			&NumberVolumesReturned);
		if (NT_SUCCESS(Status) && NumberVolumesReturned) {
			for (int i = 0; i < (int)NumberVolumesReturned; i++)
			{
				PFLT_VOLUME pVolume = ppVolumes[i];

				if (bEnable) {
					while (FltAttachVolume(gFilterHandle, pVolume, NULL, NULL) == STATUS_FLT_DELETING_OBJECT)
					{
						Interval.QuadPart = -10000000;
						KeDelayExecutionThread(0, 1u, &Interval);
					}
				}else{
					FltDetachVolume(gFilterHandle, pVolume, NULL);
				}

				FltObjectDereference(pVolume);
			}

		}
		ExFreePoolWithTag(ppVolumes, 0);
	}
	return STATUS_SUCCESS;
}

PVOID
ProcmonCollectFileOptPostInfo(
	_In_ PETHREAD Thread,
	_In_ UCHAR MajorFunction,
	_In_ FLT_CALLBACK_DATA_FLAGS Flags,
	_In_ PFLT_IO_PARAMETER_BLOCK Iopb,
	_In_ PIO_STATUS_BLOCK IoStatus,
	_In_ PULONG pLength
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	PVOID pDstBuffer = NULL;
	ULONG Length = 0xFFFF;
	if (*pLength < 0xFFFF) {
		Length = *pLength;
	}
	*pLength = Length;

	if (!IoStatus->Information && MajorFunction) {
		*pLength = 0;
		return NULL;
	}

	switch (MajorFunction)
	{
		case IRP_MJ_CREATE:
		{
			if (NT_SUCCESS(IoStatus->Status)) {
				pDstBuffer = ProcmonAllocatePoolWithTag(NonPagedPool, Length, '2');
				if (pDstBuffer) {
					ProcmonSafeCopy(TRUE, Thread, Flags, pDstBuffer, &IoStatus->Information, pLength);
				}
			}
		}
			break;
		case IRP_MJ_READ:
		{
			*pLength = Length = 8;
			pDstBuffer = ProcmonAllocatePoolWithTag(NonPagedPool, Length, '2');
			if (pDstBuffer) {
				ProcmonSafeCopy(TRUE, Thread, Flags, pDstBuffer, &IoStatus->Information, pLength);
			}
		}
			break;
		case IRP_MJ_QUERY_INFORMATION:
		case IRP_MJ_QUERY_VOLUME_INFORMATION:
		{
			if (Iopb->Parameters.QueryFileInformation.Length < 0xFFFF) {
				Length = Iopb->Parameters.QueryFileInformation.Length;
			}
			*pLength = Length;
			pDstBuffer = ProcmonAllocatePoolWithTag(NonPagedPool, Length, '2');
			if (pDstBuffer) {
				ProcmonSafeCopy(FALSE, Thread, Flags, pDstBuffer, 
					Iopb->Parameters.QueryFileInformation.InfoBuffer, pLength);
			}
		}
			break;
		case IRP_MJ_DIRECTORY_CONTROL:
		{
			if (Iopb->MinorFunction == IRP_MN_QUERY_DIRECTORY) {
				if (Iopb->Parameters.DirectoryControl.QueryDirectory.Length < 0xFFFF)
					Length = Iopb->Parameters.DirectoryControl.QueryDirectory.Length;

				*pLength = Length;
				pDstBuffer = ProcmonAllocatePoolWithTag(NonPagedPool, Length, '2');
				if (pDstBuffer){
					if (Iopb->Parameters.DirectoryControl.QueryDirectory.MdlAddress) {
						PVOID pMappedAddr = MmGetSystemAddressForMdlSafe(
							Iopb->Parameters.DirectoryControl.QueryDirectory.MdlAddress,
							NormalPagePriority);
						RtlCopyMemory(pDstBuffer, pMappedAddr, Length);
					}else{
						ProcmonSafeCopy(FALSE, Thread, Flags, pDstBuffer,
							Iopb->Parameters.DirectoryControl.QueryDirectory.DirectoryBuffer, pLength);
					}
				}
			}
		}
			break;
		case IRP_MJ_NETWORK_QUERY_OPEN:
		{
			pDstBuffer = ProcmonAllocatePoolWithTag(NonPagedPool, Length, '2');
			if (pDstBuffer) {
				ProcmonSafeCopy(FALSE, Thread, Flags, pDstBuffer,
					Iopb->Parameters.NetworkQueryOpen.NetworkInformation, pLength);
			}
		}
			break;
		default:
		{
			*pLength = 0;
		}
			break;
	}

	if (!*pLength) {
		if (pDstBuffer){
			ExFreePoolWithTag(pDstBuffer, 0);
		}

		return NULL;
	}

	if (!pDstBuffer){
		*pLength = 0;
	}

	return pDstBuffer;
}

NTSTATUS
ProcmonFilePostOptRoutine(
	_In_ PETHREAD Thread,
	_In_ UCHAR MajorFunction,
	_In_ PIO_STATUS_BLOCK IoStatus,
	_In_ PFLT_IO_PARAMETER_BLOCK Iopb,
	_In_ PVOID CompletionContext,
	_In_ LARGE_INTEGER Time,
	_In_ FLT_CALLBACK_DATA_FLAGS Flags
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	PVOID pFileOptPostInfo;
	ULONG Length;
	PLOG_BUFFER pLogBuf;
	PVOID pLogInfo;
	ULONG Seq = (ULONG)(ULONG_PTR)CompletionContext;

	if (MajorFunction == IRP_MJ_CREATE)
		Length = sizeof(IoStatus->Information);
	else
		Length = (ULONG)IoStatus->Information;

	pFileOptPostInfo = ProcmonCollectFileOptPostInfo(Thread, MajorFunction, Flags, Iopb, IoStatus, &Length);
	pLogInfo = ProcmonGetPostLogEntry(Seq, IoStatus->Status, Length, &pLogBuf);
	if (pLogInfo && Time.QuadPart) {
		PLOG_ENTRY pLogEntry = (PLOG_ENTRY)((ULONG_PTR)pLogInfo - sizeof(LOG_ENTRY));
		pLogEntry->Time = Time;
	}

	if (Length) {
		if (pLogInfo)
			RtlCopyMemory(pLogInfo, pFileOptPostInfo, Length);
		ExFreePoolWithTag(pFileOptPostInfo, 0);
	}
	if (pLogInfo){
		ProcmonNotifyProcessLog(pLogBuf);
	}
	
	return STATUS_SUCCESS;
}

VOID
ProcmonFilePostOptWorkerRoutine(
	_In_ PVOID Parameter
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	PFILEOPT_WORKQUEUEITEM pSelfWorkItem = (PFILEOPT_WORKQUEUEITEM)Parameter;
	ProcmonFilePostOptRoutine(
		pSelfWorkItem->Thread,
		pSelfWorkItem->MajorFunction,
		&pSelfWorkItem->IoStatus,
		NULL,
		pSelfWorkItem->CompletionContext,
		pSelfWorkItem->Time,
		pSelfWorkItem->Flags);
	ExFreePoolWithTag(pSelfWorkItem, 0);
}