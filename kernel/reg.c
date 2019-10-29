
#include "reg.h"
#include "globals.h"
#include "utils.h"
#include "log.h"
#include "process.h"


BOOLEAN gbRegCallbackSet;
LARGE_INTEGER gCookie;
PAGED_LOOKASIDE_LIST gLookasideRegPostInfo;
KMUTEX gMutexRegPostInfo;
LIST_ENTRY gRegPostInfoList;
KMUTEX gMutexRegObjectList;
LIST_ENTRY gRegObjectList;

FNCmRegisterCallback fnCmRegisterCallback;
FNCmRegisterCallbackEx fnCmRegisterCallbackEx;
FNCmUnRegisterCallback fnCmUnRegisterCallback;
FNCmCallbackGetKeyObjectID fnCmCallbackGetKeyObjectID;

UNICODE_STRING gUniStrDefault;
UNICODE_STRING gUniStrInsufficentRef;
UNICODE_STRING gUniRegistry;
UNICODE_STRING gUniInvalidName;

VOID
ProcmonAddToPrePostList(
	_In_ ULONG RecordSequence,
	_In_ PVOID pRegData
)
{
	PREG_POST_INFO pRegPostInfo;

	pRegPostInfo = (PREG_POST_INFO)ExAllocateFromPagedLookasideList(&gLookasideRegPostInfo);
	if (pRegPostInfo) {
		pRegPostInfo->pRegData = pRegData;
		pRegPostInfo->Thread = KeGetCurrentThread();
		pRegPostInfo->Seq = RecordSequence;

		KeWaitForSingleObject(&gMutexRegPostInfo, 0, 0, 0, NULL);
		InsertHeadList(&gRegPostInfoList, &pRegPostInfo->List);
		KeReleaseMutex(&gMutexRegPostInfo, FALSE);
	}
}

PREG_POST_INFO
ProcmonRegGetPrePostInfo(
	VOID
)
{
	PREG_POST_INFO pRegPostInfo = NULL;
	PETHREAD Thread = KeGetCurrentThread();

	KeWaitForSingleObject(&gMutexRegPostInfo, 0, 0, 0, NULL);
	if (!IsListEmpty(&gRegPostInfoList)) {
		PLIST_ENTRY pEntry;

		for (pEntry = gRegPostInfoList.Flink;
			pEntry != &gRegPostInfoList;
			pEntry = pEntry->Flink)
		{
			PREG_POST_INFO pRegPostInfoTemp = CONTAINING_RECORD(pEntry, REG_POST_INFO, List);
			if (pRegPostInfoTemp->Thread == Thread) {
				pRegPostInfo = pRegPostInfoTemp;
				break;
			}
		}

		if (pRegPostInfo) {
			RemoveEntryList(pEntry);
		}
	}
	KeReleaseMutex(&gMutexRegPostInfo, FALSE);
	return pRegPostInfo;
}

VOID
ProcmonFreePostInfo(
	_In_ PREG_POST_INFO pPostInfo
)
{
	ExFreeToPagedLookasideList(&gLookasideRegPostInfo, pPostInfo);
}

VOID
ProcmonRegAddObjectNameToList(
	_In_ PVOID Object,
	_In_ PUNICODE_STRING pUniStrObjectName
)
{
	BOOLEAN bFind = FALSE;
	PREG_OBJECT_INFO pObjectNameInfo;

	KeWaitForSingleObject(&gMutexRegObjectList, 0, 0, 0, NULL);

	if (!IsListEmpty(&gRegObjectList)) {

		//
		// Try to find the object
		//

		PLIST_ENTRY pEntry;
		for (pEntry = gRegObjectList.Flink;
			pEntry != &gRegObjectList;
			pEntry = pEntry->Flink)
		{
			pObjectNameInfo = CONTAINING_RECORD(pEntry, REG_OBJECT_INFO, List);
			if (pObjectNameInfo->Object == Object) {
				bFind = TRUE;
				if (pObjectNameInfo->Name != pUniStrObjectName) {
					ExFreePoolWithTag(pUniStrObjectName, 0);
				}
				break;
			}
		}
	}

	if (!bFind) {

		//
		// Allocate a new buffer to save the object
		//

		pObjectNameInfo = (PREG_OBJECT_INFO)ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(REG_OBJECT_INFO), '4');
		if (pObjectNameInfo) {
			pObjectNameInfo->Object = Object;
			pObjectNameInfo->Name = pUniStrObjectName;

			InsertHeadList(&gRegObjectList, &pObjectNameInfo->List);
		}
	}

	KeReleaseMutex(&gMutexRegObjectList, 0);

}

PUNICODE_STRING
ProcmonRegFindObjectNameFromList(
	_In_ PVOID Object
)
{
	PLIST_ENTRY pEntry;
	PUNICODE_STRING pRet = NULL;

	if (!Object){
		return NULL;
	}

	KeWaitForSingleObject(&gMutexRegObjectList, 0, 0, 0, NULL);

	for (pEntry = gRegObjectList.Flink;
		pEntry != &gRegObjectList;
		pEntry = pEntry->Flink)
	{
		PREG_OBJECT_INFO pRegObjInfo = CONTAINING_RECORD(pEntry, REG_OBJECT_INFO, List);
		if (pRegObjInfo->Object == Object) {
			pRet = pRegObjInfo->Name;
			break;
		}
	}
	KeReleaseMutex(&gMutexRegObjectList, 0);

	return NULL;
}

PUNICODE_STRING
ProcmonRegFindObjectNameFromListByNotifyType(
	_In_ USHORT NotifyType,
	_In_ PVOID Object
)
{
	PLIST_ENTRY pEntry;
	PREG_OBJECT_INFO pRegObjInfo = NULL;
	PUNICODE_STRING pRet = NULL;

	KeWaitForSingleObject(&gMutexRegObjectList, 0, 0, 0, NULL);

	for (pEntry = gRegObjectList.Flink;
		pEntry != &gRegObjectList;
		pEntry = pEntry->Flink)
	{
		PREG_OBJECT_INFO pRegObjInfoTmp = CONTAINING_RECORD(pEntry, REG_OBJECT_INFO, List);
		if (pRegObjInfoTmp->Object == Object) {
			pRegObjInfo = pRegObjInfoTmp;
			break;
		}
	}

	if (pRegObjInfo){
		pRet = pRegObjInfo->Name;
		if (NotifyType == NOTIFY_REG_KEYHANDLECLOSE) {
			RemoveEntryList(pEntry);
			ExFreePoolWithTag(pRegObjInfo, 0);
		}
	}

	KeReleaseMutex(&gMutexRegObjectList, 0);

	return pRet;
}

LONG CleanUpAllRegPostInfoList()
{
	PLIST_ENTRY pEntry;

	KeWaitForSingleObject(&gMutexRegPostInfo, 0, 0, 0, NULL);
	for (pEntry = gRegPostInfoList.Flink;
		gRegPostInfoList.Flink != &gRegPostInfoList;
		pEntry = gRegPostInfoList.Flink)
	{

		PREG_POST_INFO pRegPostInfo = CONTAINING_RECORD(pEntry, REG_POST_INFO, List);
		RemoveHeadList(&gRegPostInfoList);
		//RemoveEntryList(pEntry);

		ExFreePoolWithTag(pRegPostInfo, 0);
	}
	return KeReleaseMutex(&gMutexRegPostInfo, 0);
}

LONG CleanupRegObjectList()
{
	PLIST_ENTRY pEntry;

	KeWaitForSingleObject(&gMutexRegObjectList, 0, 0, 0, NULL);
	for (pEntry = gRegObjectList.Flink;
		gRegObjectList.Flink != &gRegObjectList;
		pEntry = gRegObjectList.Flink)
	{
		PREG_OBJECT_INFO pRegObjInfo = CONTAINING_RECORD(pEntry, REG_OBJECT_INFO, List);
		RemoveHeadList(&gRegPostInfoList);
		ExFreePoolWithTag(pRegObjInfo->Name, 0);
		ExFreePoolWithTag(pRegObjInfo, 0);
	}
	return KeReleaseMutex(&gMutexRegObjectList, 0);
}

USHORT
ProcmonGetTypeMaxSize(
	_In_ ULONG Type,
	_In_ PVOID Data,
	_In_ ULONG DataSize
)
{
	USHORT MaxSize;

	UNREFERENCED_PARAMETER(Data);

	switch (Type)
	{
	case REG_NONE:
	case REG_BINARY:
		MaxSize = 0x10;
		if (DataSize < 0x10)
			MaxSize = (USHORT)DataSize;
		break;
	case REG_SZ:
	case REG_EXPAND_SZ:
	case REG_MULTI_SZ:
		MaxSize = 0x800;
		if (DataSize < 0x800)
			MaxSize = (USHORT)DataSize;
		break;
	case REG_DWORD:
		MaxSize = 4;
		break;
	case REG_QWORD:
		MaxSize = 8;
		break;
	default:
		MaxSize = 0;
		break;
	}
	return MaxSize;
}

PUNICODE_STRING
ProcmonQueryObjectFullNameByObject(
	_In_ BOOLEAN bDefaultToNull,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName
)
{
	NTSTATUS Status;
	PUNICODE_STRING pUniStrObjectName = NULL;
	PUNICODE_STRING pUniStrValueName = NULL;
	PUNICODE_STRING pUniStrFullName = NULL;

	if (ValueName) {

		
// 		try{
// 			if (ValueName->Length) {
// 				pUniStrValueName = ValueName;
// 			}else {
// 				pUniStrValueName = bDefaultToNull ? NULL : &gUniStrDefault;
// 			}
// 		}except(EXCEPTION_EXECUTE_HANDLER){
// 			pUniStrValueName = &gUniInvalidName;
// 		}

		try{
			if (ValueName->Length) {
				pUniStrValueName = ProcmonDuplicateUnicodeString(NonPagedPool, ValueName, 'R');
			}else{
				pUniStrValueName = bDefaultToNull ? NULL : &gUniStrDefault;
			}
		}except(EXCEPTION_EXECUTE_HANDLER){
			pUniStrValueName = &gUniInvalidName;
		}

	}

	if (Object) {

		if (fnCmCallbackGetKeyObjectID) {
			Status = fnCmCallbackGetKeyObjectID(&gCookie, Object, NULL, &pUniStrObjectName);
			if (NT_SUCCESS(Status)) {
				ULONG Length = pUniStrObjectName->Length + sizeof(UNICODE_STRING);
				if (pUniStrValueName) {
					Length += pUniStrValueName->Length + 4;
				}

				pUniStrFullName = (PUNICODE_STRING)ProcmonAllocatePoolWithTag(NonPagedPool, Length, 'D');
				if (pUniStrFullName) {
					pUniStrFullName->Length = 0;
					pUniStrFullName->MaximumLength = (USHORT)Length - sizeof(UNICODE_STRING);
					pUniStrFullName->Buffer = (PWCH)(pUniStrFullName + 1);

					//
					// Copy the object name to full name
					//

					RtlCopyUnicodeString(pUniStrFullName, pUniStrObjectName);
				}
			}
		}else{
			POBJECT_NAME_INFORMATION pObjNameInfo;
			ULONG ReturnLength;
			ULONG Length;
			Status = ObQueryNameString(Object, NULL, 0, &ReturnLength);
			if (Status == STATUS_INFO_LENGTH_MISMATCH) {
				Length = ReturnLength;
				if (pUniStrValueName) {
					Length += pUniStrValueName->Length + 4;
				}

				pObjNameInfo = (POBJECT_NAME_INFORMATION)ProcmonAllocatePoolWithTag(NonPagedPool, Length, 'D');
				if (pObjNameInfo) {
					pUniStrFullName = &pObjNameInfo->Name;
					Status = ObQueryNameString(Object, pObjNameInfo, ReturnLength, &ReturnLength);
					if (NT_SUCCESS(Status)) {
						if (pUniStrValueName) {
							pUniStrFullName->MaximumLength = (USHORT)Length;
						}
					}else{
						ExFreePoolWithTag(pObjNameInfo, 0);
						pUniStrFullName = &gUniStrInsufficentRef;
					}
				}
			}
		}

		if (pUniStrFullName && !RtlCompareUnicodeString(pUniStrFullName, &gUniRegistry, TRUE)) {
			if (pUniStrFullName != &gUniStrInsufficentRef)
				ExFreePoolWithTag(pUniStrFullName, 0);
			pUniStrFullName = NULL;
		}
	}

	if (pUniStrValueName) {
		if (pUniStrFullName) {
			if (pUniStrFullName != &gUniStrInsufficentRef) {
				UNICODE_STRING UniStrBackslash;
				RtlInitUnicodeString(&UniStrBackslash, L"\\");
				RtlAppendUnicodeStringToString(pUniStrFullName, &UniStrBackslash);
				RtlAppendUnicodeStringToString(pUniStrFullName, pUniStrValueName);
			}
		}
		else
		{
			PUNICODE_STRING pUniName = (PUNICODE_STRING)ProcmonAllocatePoolWithTag(NonPagedPool,
				pUniStrValueName->Length + 0x10, 'E');
			if (pUniName) {
				pUniName->Buffer = (PWCH)(pUniName + 1);
				pUniName->MaximumLength = pUniStrValueName->Length;
				RtlCopyUnicodeString(pUniName, pUniStrValueName);

				pUniStrFullName = pUniName;
			}else {
				pUniStrFullName = &gUniStrInsufficentRef;
			}
		}
	}

	if (!pUniStrFullName) {
		pUniStrFullName = &gUniInvalidName;
	}

	if (pUniStrFullName->Buffer) {
		if (!pUniStrValueName && pUniStrFullName->Buffer[(pUniStrFullName->Length >> 1) - 1] == L'\\') {
			pUniStrFullName->Length -= sizeof(WCHAR);
		}
	}

	if (pUniStrValueName && pUniStrValueName != &gUniInvalidName && pUniStrValueName != &gUniStrDefault) {
		ExFreePoolWithTag(pUniStrValueName, 0);
	}

	return pUniStrFullName;
}


PUNICODE_STRING
ProcmonQueryObjectFullName(
	_In_ BOOLEAN bDefaultToNull,
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName
)
{
	PUNICODE_STRING pUniStrObjName;
	if (Handle) {
		Object = ObReferenceObjectByHandleSafe(Handle);
	}

	pUniStrObjName = ProcmonQueryObjectFullNameByObject(bDefaultToNull, Object, ValueName);

	if (Handle && Object) {
		ObDereferenceObject(Object);
	}

	return pUniStrObjName;
}

LONG
ProcmonNotifyRegSetValueKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName,
	_In_ ULONG TitleIndex,
	_In_ ULONG Type,
	_In_ PVOID Data,
	_In_ ULONG DataSize
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	PVOID pDataCopy = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_SETVALUEKEY pRegLogSetValueKey = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	UNREFERENCED_PARAMETER(TitleIndex);

	USHORT CopySize = ProcmonGetTypeMaxSize(Type, Data, DataSize);
	if (CopySize) {
		CopySize = (USHORT)ProcmonDuplicateUserBuffer(Data, CopySize, &pDataCopy);
	}

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}else{

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, ValueName);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + CopySize + sizeof(LOG_REG_SETVALUEKEY);
				pRegLogSetValueKey = (PLOG_REG_SETVALUEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_SETVALUEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegLogSetValueKey) {
					pRegLogSetValueKey->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegLogSetValueKey + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegLogSetValueKey) {
				pRegLogSetValueKey->Type = Type;
				pRegLogSetValueKey->DataSize = DataSize;
				pRegLogSetValueKey->CopySize = 0;
				if (CopySize) {
					PVOID pLogBufEnd = (PVOID)((ULONG_PTR)pRegLogSetValueKey + sizeof(LOG_REG_SETVALUEKEY) +
						sizeof(WCHAR) * pRegLogSetValueKey->KeyNameLength);
					if (pDataCopy) {
						pRegLogSetValueKey->CopySize = CopySize;
						RtlCopyMemory(pLogBufEnd, pDataCopy, CopySize);
					}
					else {
						RtlZeroMemory(pLogBufEnd, CopySize);
					}
				}
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	if (pDataCopy) {
		ExFreePoolWithTag(pDataCopy, 0);
	}

	return Seq;
}

LONG
ProcmonNotifyRegDeleteValueKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_DELETEVALUEKEY pRegLogDeleteValueKey = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, ValueName);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_DELETEVALUEKEY);
				pRegLogDeleteValueKey = (PLOG_REG_DELETEVALUEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_DELETEVALUEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegLogDeleteValueKey) {
					pRegLogDeleteValueKey->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegLogDeleteValueKey + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegLogDeleteValueKey) {
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}

LONG
ProcmonNotifyRegSetInformationKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ KEY_SET_INFORMATION_CLASS KeySetInformationClass,
	_In_ PVOID KeySetInformation,
	_In_ ULONG KeySetInformationLength
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	PVOID pDataCopy = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_SETINFORMATIONKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;
	USHORT CopySize = 0;

	if (KeySetInformationClass == KeyWriteTimeInformation) {
		CopySize = sizeof(KEY_WRITE_TIME_INFORMATION);
	}
	else if (KeySetInformationClass == KeyWow64FlagsInformation) {
		CopySize = sizeof(ULONG);
	}

	if (CopySize) {
		CopySize = (USHORT)ProcmonDuplicateUserBuffer(KeySetInformation, CopySize, &pDataCopy);
	}

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, NULL);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + CopySize + sizeof(LOG_REG_SETINFORMATIONKEY);
				pRegOptInfo = (PLOG_REG_SETINFORMATIONKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_SETINFORMATIONKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				pRegOptInfo->KeySetInformationClass = KeySetInformationClass;
				pRegOptInfo->KeySetInformationLength = KeySetInformationLength;
				pRegOptInfo->CopySize = 0;
				if (CopySize) {
					PVOID pLogBufEnd = (PVOID)((ULONG_PTR)pRegOptInfo + sizeof(LOG_REG_SETINFORMATIONKEY) +
						sizeof(WCHAR) * pRegOptInfo->KeyNameLength);
					if (pDataCopy) {
						pRegOptInfo->CopySize = CopySize;
						RtlCopyMemory(pLogBufEnd, pDataCopy, CopySize);
					}
					else {
						RtlZeroMemory(pLogBufEnd, CopySize);
					}
				}
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	if (pDataCopy) {
		ExFreePoolWithTag(pDataCopy, 0);
	}

	return Seq;
}

LONG
ProcmonNotifyRegRenameKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING NewName
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_RENAMEKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, NULL);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + NewName->Length + sizeof(LOG_REG_RENAMEKEY);
				pRegOptInfo = (PLOG_REG_RENAMEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_RENAMEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				PVOID pLogBufEnd;

				pRegOptInfo->NewNameLength = NewName->Length;
				pLogBufEnd = (PVOID)((ULONG_PTR)pRegOptInfo + sizeof(LOG_REG_RENAMEKEY) +
					sizeof(WCHAR) * pRegOptInfo->KeyNameLength);
				RtlCopyMemory(pLogBufEnd, NewName->Buffer, NewName->Length);

				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}


LONG
ProcmonNotifyRegEnumerateKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ ULONG Index,
	_In_ KEY_INFORMATION_CLASS KeyInformationClass,
	_In_ ULONG Length
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_ENUMERATEKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}else{

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, NULL);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_ENUMERATEKEY);
				pRegOptInfo = (PLOG_REG_ENUMERATEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_ENUMERATEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				pRegOptInfo->Index = Index;
				pRegOptInfo->KeyInformationClass = KeyInformationClass;
				pRegOptInfo->Length = Length;
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}

LONG
ProcmonNotifyRegEnumerateValueKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ ULONG Index,
	_In_ KEY_VALUE_INFORMATION_CLASS KeyValueInformationClass,
	_In_ ULONG Length
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_ENUMERATEVALUEKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, NULL);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_ENUMERATEVALUEKEY);
				pRegOptInfo = (PLOG_REG_ENUMERATEVALUEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_ENUMERATEVALUEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				pRegOptInfo->Index = Index;
				pRegOptInfo->KeyValueInformationClass = KeyValueInformationClass;
				pRegOptInfo->Length = Length;
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}


LONG
ProcmonNotifyRegQueryKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ KEY_INFORMATION_CLASS KeyInformationClass,
	_In_ ULONG Length
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_QUERYKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, NULL);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_QUERYKEY);
				pRegOptInfo = (PLOG_REG_QUERYKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_QUERYKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				pRegOptInfo->KeyInformationClass = KeyInformationClass;
				pRegOptInfo->Length = Length;
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}

LONG
ProcmonNotifyRegQueryValueKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName,
	_In_ KEY_VALUE_INFORMATION_CLASS KeyValueInformationClass,
	_In_ ULONG Length
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	BOOLEAN bNameFind = FALSE;
	PLOG_REG_QUERYVALUEKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	if (gFlags & 0xc) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
		if (pProcessInfo) {
			if (Object) {

				//
				// Find Object name from list
				//

				pUniStrObjName = ProcmonRegFindObjectNameFromList(Object);
			}

			if (pUniStrObjName) {
				bNameFind = TRUE;
			}
			else {

				//
				// Try to query Object name
				//

				pUniStrObjName = ProcmonQueryObjectFullName(FALSE, Handle, Object, ValueName);
			}

			if (pUniStrObjName) {
				ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_QUERYVALUEKEY);
				pRegOptInfo = (PLOG_REG_QUERYVALUEKEY)ProcmonGetLogEntryAndSeq(
					TRUE,
					MONITOR_TYPE_REG,
					NOTIFY_REG_QUERYVALUEKEY,
					pProcessInfo->Seq,
					STATUS_PENDING,
					LogBufSize,
					&Seq,
					&pLogBuf);

				if (pRegOptInfo) {
					pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
					RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
				}

				if (!bNameFind && pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
					ExFreePoolWithTag(pUniStrObjName, 0);
				}
			}

			DerefProcessInfo(pProcessInfo);
			if (pRegOptInfo) {
				pRegOptInfo->KeyValueInformationClass = KeyValueInformationClass;
				pRegOptInfo->Length = Length;
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}
	}

	return Seq;
}


LONG
ProcmonNotifyRegLoadKey(
	_In_ PVOID Object,
	_In_ PUNICODE_STRING KeyName,
	_In_ PUNICODE_STRING SourceFile
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	PLOG_REG_LOADKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
	if (pProcessInfo) {
		if (Object)
			pUniStrObjName = ProcmonQueryObjectFullNameByObject(TRUE, Object, KeyName);
		else
			pUniStrObjName = KeyName;
		pRegOptInfo = (PLOG_REG_LOADKEY)ProcmonGetLogEntryAndSeq(TRUE, MONITOR_TYPE_REG, NOTIFY_REG_LOADKEY, pProcessInfo->Seq,
			STATUS_PENDING, pUniStrObjName->Length + SourceFile->Length + sizeof(LOG_REG_LOADKEY),
			&Seq, &pLogBuf);
		if (pRegOptInfo) {
			pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
			RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
			pRegOptInfo->SourceFileLength = SourceFile->Length >> 1;
			RtlCopyMemory((PVOID)((ULONG_PTR)pRegOptInfo + pUniStrObjName->Length + sizeof(LOG_REG_LOADKEY)),
				SourceFile->Buffer,
				SourceFile->Length);
			ProcmonNotifyProcessLog(pLogBuf);
		}
		if (pUniStrObjName != KeyName &&
			pUniStrObjName != &gUniInvalidName &&
			pUniStrObjName != &gUniStrInsufficentRef) {
			ExFreePoolWithTag(pUniStrObjName, 0);
		}
		DerefProcessInfo(pProcessInfo);
	}
	return Seq;
}

LONG
ProcmonNotifyRegUnLoadKey(
	_In_ HANDLE Handle,
	_In_ PVOID Object
)
{
	PUNICODE_STRING pUniStrObjName = NULL;
	LONG Seq = -1;
	PLOG_REG_UNLOADKEY pRegOptInfo = NULL;
	PLOG_BUFFER pLogBuf = NULL;

	PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
	if (pProcessInfo) {
		if (!Handle)
			pUniStrObjName = ProcmonQueryObjectFullNameByObject(TRUE, Object, NULL);
		pRegOptInfo = (PLOG_REG_UNLOADKEY)ProcmonGetLogEntryAndSeq(TRUE, MONITOR_TYPE_REG, NOTIFY_REG_UNLOADKEY, pProcessInfo->Seq,
			STATUS_PENDING, pUniStrObjName->Length + sizeof(LOG_REG_UNLOADKEY),
			&Seq, &pLogBuf);
		if (pRegOptInfo) {
			pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
			RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
			ProcmonNotifyProcessLog(pLogBuf);
		}
		if (pUniStrObjName != &gUniInvalidName &&
			pUniStrObjName != &gUniStrInsufficentRef) {
			ExFreePoolWithTag(pUniStrObjName, 0);
		}
		DerefProcessInfo(pProcessInfo);
	}
	return Seq;
}

BOOLEAN
ProcmonSelfReadUnicodeString(
	_In_ PUNICODE_STRING pUniStrSrc,
	_Out_ PUNICODE_STRING pUniStrRead)
{

	try {
		if (ExGetPreviousMode() == UserMode)
		{
			ProbeForRead(pUniStrSrc, sizeof(UNICODE_STRING), 1);
			*pUniStrRead = *pUniStrSrc;
			ProbeForRead(pUniStrRead->Buffer, pUniStrRead->Length, 1u);
		}
		else {
			*pUniStrRead = *pUniStrSrc;
		}
	}
	except (EXCEPTION_EXECUTE_HANDLER) {
		*pUniStrRead = gUniInvalidName;
	}
	return TRUE;
}

BOOLEAN
ProcmonSafeReadObjectAttributes(
	_In_ POBJECT_ATTRIBUTES pObjectAttribute,
	_Out_ PHANDLE phRoot,
	_Out_ PUNICODE_STRING pObjectName)
{
	try {

		if (ExGetPreviousMode() == UserMode) {
			ProbeForRead(pObjectAttribute, sizeof(OBJECT_ATTRIBUTES), 1);
			ProcmonSelfReadUnicodeString(pObjectAttribute->ObjectName, pObjectName);
		}
		else {
			*pObjectName = *pObjectAttribute->ObjectName;
		}

		*phRoot = pObjectAttribute->RootDirectory;
	}
	except (EXCEPTION_EXECUTE_HANDLER) {
		return FALSE;
	}
	return TRUE;
}

LONG
ProcmonNotifyRegCreateOpenKeyEx(
	_In_ USHORT NotifyType,
	_In_ ACCESS_MASK DesiredAccess,
	_In_ POBJECT_ATTRIBUTES pObjectAttribute,
	_In_ PVOID RootObject,
	_In_ PUNICODE_STRING CompleteName
)
{
	LONG Seq = -1;
	PUNICODE_STRING pUniStrObjName = NULL;
	PLOG_BUFFER pLogBuf;
	PLOG_REG_CREATEOPENKEY pRegOptInfo = NULL;

	PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
	if (!pProcessInfo) {
		return -1;
	}

	if (pObjectAttribute) {
		HANDLE hRoot;
		UNICODE_STRING ObjectName;
		PVOID Object = NULL;
		if (ProcmonSafeReadObjectAttributes(pObjectAttribute, &hRoot, &ObjectName)) {
			if (hRoot) {
				Object = ObReferenceObjectByHandleSafe(hRoot);
			}
			pUniStrObjName = ProcmonQueryObjectFullNameByObject(TRUE, Object, &ObjectName);

			if (Object) {
				ObDereferenceObject(Object);
			}
		}
	}
	else {
		pUniStrObjName = ProcmonQueryObjectFullNameByObject(TRUE, RootObject, CompleteName);
	}

	if (pUniStrObjName) {
		ULONG LogBufSize = pUniStrObjName->Length + sizeof(LOG_REG_CREATEOPENKEY);
		pRegOptInfo = (PLOG_REG_CREATEOPENKEY)ProcmonGetLogEntryAndSeq(
			TRUE,
			MONITOR_TYPE_REG,
			NotifyType,
			pProcessInfo->Seq,
			STATUS_PENDING,
			LogBufSize,
			&Seq,
			&pLogBuf);

		if (pRegOptInfo) {
			pRegOptInfo->KeyNameLength = pUniStrObjName->Length >> 1;
			pRegOptInfo->DesiredAccess = DesiredAccess;
			RtlCopyMemory(pRegOptInfo + 1, pUniStrObjName->Buffer, pUniStrObjName->Length);
			ProcmonNotifyProcessLog(pLogBuf);
		}

		if (pUniStrObjName != &gUniInvalidName && pUniStrObjName != &gUniStrInsufficentRef) {
			ExFreePoolWithTag(pUniStrObjName, 0);
		}
	}

	DerefProcessInfo(pProcessInfo);
	return Seq;

}

PVOID
ProcmonNotifyRegOptCommonInternal(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ PUNICODE_STRING ValueName,
	_In_ USHORT NotifyType,
	_In_ ULONG HeaderLength,
	_In_ ULONG ExtendLength,
	_Out_ PLOG_BUFFER* ppLogBuffer,
	_Out_ PLONG pRecordSequence
)
{
	BOOLEAN bDefaultToNull = FALSE;
	BOOLEAN bNameFind = FALSE;
	PPROCESSINFO_LIST pProcessInfo;
	PUNICODE_STRING pUniStrObjectName = NULL;
	PLOG_REG_CONNMON pRegOptInfo = NULL;

	*ppLogBuffer = NULL;


	//
	// add by progmboy
	//

	if (HeaderLength < 2) {
		return NULL;
	}

	//
	// Is register monitor closed
	//

	if (!(gFlags & 0xc)) {
		return NULL;
	}

	pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
	if (!pProcessInfo) {
		return NULL;
	}


	if (NotifyType <= 1 || NotifyType <= 0xd) {
		bDefaultToNull = TRUE;
	}

	pUniStrObjectName = ProcmonRegFindObjectNameFromListByNotifyType(NotifyType, Object);
	if (pUniStrObjectName) {
		bNameFind = TRUE;
	}
	else {
		pUniStrObjectName = ProcmonQueryObjectFullName(bDefaultToNull, Handle, Object, ValueName);
	}

	if (pUniStrObjectName) {
		ULONG LogBufSize = pUniStrObjectName->Length + HeaderLength + ExtendLength;
		pRegOptInfo = (PLOG_REG_CONNMON)ProcmonGetLogEntryAndSeq(
			TRUE,
			MONITOR_TYPE_REG,
			NotifyType,
			pProcessInfo->Seq,
			STATUS_PENDING,
			LogBufSize,
			pRecordSequence,
			ppLogBuffer);

		if (pRegOptInfo) {
			pRegOptInfo->KeyNameLength = pUniStrObjectName->Length >> 1;
			RtlCopyMemory((PVOID)((ULONG_PTR)pRegOptInfo + HeaderLength), pUniStrObjectName->Buffer, pUniStrObjectName->Length);
		}

		if (NotifyType == NOTIFY_REG_DELETEKEY && Object) {
			ProcmonRegAddObjectNameToList(Object, pUniStrObjectName);
		}else if ((!bNameFind || NotifyType == NOTIFY_REG_KEYHANDLECLOSE)
			&& pUniStrObjectName != &gUniInvalidName
			&& pUniStrObjectName != &gUniStrInsufficentRef) {
			ExFreePoolWithTag(pUniStrObjectName, 0);
		}
	}

	DerefProcessInfo(pProcessInfo);
	return pRegOptInfo;

}

LONG
ProcmonNotifyRegOptCommon(
	_In_ HANDLE Handle,
	_In_ PVOID Object,
	_In_ USHORT NotifyType,
	_In_ ULONG HeadLength
)
{
	PLOG_BUFFER pLogBuffer;
	LONG RecordSequence = -1;

	if (ProcmonNotifyRegOptCommonInternal(Handle, Object, NULL, NotifyType, HeadLength, 0, &pLogBuffer, &RecordSequence))
		ProcmonNotifyProcessLog(pLogBuffer);
	return RecordSequence;
}

VOID
ProcmonNotifyPostRegEnumerateKey(
	_In_ LONG Seq,
	_In_ NTSTATUS Status,
	_In_ KEY_INFORMATION_CLASS KeyInformationClass,
	_In_ PVOID KeyInformation,
	_In_ PULONG ResultLength
)
{
	PLOG_BUFFER pLogBuf = NULL;
	PVOID pLogInfo = NULL;

	UNREFERENCED_PARAMETER(KeyInformationClass);

	if (Seq == -1) {
		return;
	}

	if (!NT_SUCCESS(Status)) {
		ProcmonGetPostLogEntry(Seq, Status, 0, &pLogBuf);
	}else{
		ULONG Length;
		PVOID pBufferDup = NULL;

		try {
			Length = *ResultLength;
		}except(EXCEPTION_EXECUTE_HANDLER) {
			return;
		}

		if (Length) {
			Length = ProcmonDuplicateUserBuffer(KeyInformation, (USHORT)Length, &pBufferDup);
		}

		pLogInfo = ProcmonGetPostLogEntry(Seq, Status, Length, &pLogBuf);
		if (pLogInfo) {
			if (pBufferDup) {
				RtlCopyMemory(pLogInfo, pBufferDup, Length);
				ExFreePoolWithTag(pBufferDup, 0);
			}
			else {
				RtlZeroMemory(pLogInfo, Length);
			}
		}
	}
	if (pLogBuf) {
		ProcmonNotifyProcessLog(pLogBuf);
	}

}

USHORT
ProcmonDupQueryValueKeyInformation(
	_In_ KEY_VALUE_INFORMATION_CLASS KeyValueInformationClass,
	_In_ PVOID KeyValueInformation,
	_In_ ULONG Length,
	_Out_ PVOID *ppBufferDup
)
{
	ULONG DstLength;

	UNREFERENCED_PARAMETER(Length);

	
	//
	// Kernel buffer is a user buffer we need try
	//
	
	try {
		switch (KeyValueInformationClass)
		{
		case KeyValueBasicInformation:
			DstLength = sizeof(KEY_VALUE_BASIC_INFORMATION);
			break;
		case KeyValueFullInformation:
		{
			PKEY_VALUE_FULL_INFORMATION pFullInfo = (PKEY_VALUE_FULL_INFORMATION)KeyValueInformation;
			DstLength = ProcmonGetTypeMaxSize(
				pFullInfo->Type,
				(PVOID)((ULONG_PTR)pFullInfo + pFullInfo->DataOffset),
				pFullInfo->DataLength) + pFullInfo->DataOffset;
		}
			break;
		case KeyValuePartialInformation:
		{
			PKEY_VALUE_PARTIAL_INFORMATION pPartialInfo = (PKEY_VALUE_PARTIAL_INFORMATION)KeyValueInformation;
			DstLength = ProcmonGetTypeMaxSize(
				pPartialInfo->Type,
				&pPartialInfo->Data[0],
				pPartialInfo->DataLength) + FIELD_OFFSET(KEY_VALUE_PARTIAL_INFORMATION, Data);
		}
			break;
		default:
			return 0;
		}
	}except(EXCEPTION_EXECUTE_HANDLER){
		*ppBufferDup = NULL;
		return 0;
	}


	return ProcmonDuplicateUserBuffer(KeyValueInformation, (USHORT)DstLength, ppBufferDup);
}


VOID
ProcmonNotifyPostRegEnumerateValueKey(
	_In_ LONG Seq,
	_In_ NTSTATUS Status,
	_In_ KEY_VALUE_INFORMATION_CLASS KeyValueInformationClass,
	_In_ PVOID KeyValueInformation,
	_In_ PULONG ResultLength
)
{
	PLOG_BUFFER pLogBuf = NULL;

	if (Seq == -1) {
		return;
	}

	if (!NT_SUCCESS(Status)) {
		ProcmonGetPostLogEntry(Seq, Status, 0, &pLogBuf);
	}else{
		USHORT CopyLength;
		PVOID pDataDup = NULL;
		PVOID pLogInfo;
		ULONG Length;

		try {
			Length = *ResultLength;
		}except(EXCEPTION_EXECUTE_HANDLER) {
			return;
		}

		CopyLength = ProcmonDupQueryValueKeyInformation(KeyValueInformationClass, KeyValueInformation, Length, &pDataDup);
		pLogInfo = ProcmonGetPostLogEntry(Seq, Status, CopyLength, &pLogBuf);

		if (pLogInfo) {
			if (pDataDup) {
				RtlCopyMemory(pLogInfo, pDataDup, CopyLength);
				ExFreePoolWithTag(pDataDup, 0);
			}
			else {
				RtlZeroMemory(pLogInfo, CopyLength);
			}
		}
	}

	if (pLogBuf) {
		ProcmonNotifyProcessLog(pLogBuf);
	}
}

VOID
ProcmonNotifyPostRegCreateOpenKey(
	_In_ LONG Seq,
	_In_ NTSTATUS Status,
	_In_ ACCESS_MASK DesiredAccess,
	_In_ ACCESS_MASK GrantedAccess,
	_In_ ULONG Disposition)
{
	PLOG_BUFFER pLogBuf = NULL;
	ULONG Length = 0;
	PVOID pLogInfo;

	UNREFERENCED_PARAMETER(DesiredAccess);

	if (Seq == -1) {
		return;
	}

	if (NT_SUCCESS(Status))
		Length = sizeof(LOG_REG_POSTCREATEOPENKEY);

	pLogInfo = ProcmonGetPostLogEntry(Seq, Status, Length, &pLogBuf);
	if (pLogInfo) {
		if (Length) {
			LOG_REG_POSTCREATEOPENKEY PostCreateOpenKey;
			PostCreateOpenKey.GrantedAccess = GrantedAccess;
			PostCreateOpenKey.Disposition = Disposition;

			RtlCopyMemory(pLogInfo, &PostCreateOpenKey, Length);
		}
	}

	if (pLogBuf) {
		ProcmonNotifyProcessLog(pLogBuf);
	}
}

NTSTATUS
ProcmonRegistryCallback(
	_In_      PVOID CallbackContext,
	_In_opt_  PVOID Argument1,
	_In_opt_  PVOID Argument2
)
{
	USHORT NotifyType;
	LONG Seq;
	REG_NOTIFY_CLASS RegNotifyClass = (REG_NOTIFY_CLASS)(ULONG_PTR)Argument1;
	PLOG_BUFFER pLogBuffer = NULL;

	UNREFERENCED_PARAMETER(CallbackContext);

	switch (RegNotifyClass)
	{
	case RegNtPreDeleteKey:
		NotifyType = NOTIFY_REG_DELETEKEY;
		goto __label_reg_pre_common;
	case RegNtPreSetValueKey:
	{
		PREG_SET_VALUE_KEY_INFORMATION pRegSetValueInfo = (PREG_SET_VALUE_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegSetValueKey(NULL, pRegSetValueInfo->Object,
			pRegSetValueInfo->ValueName, pRegSetValueInfo->TitleIndex,
			pRegSetValueInfo->Type, pRegSetValueInfo->Data, pRegSetValueInfo->DataSize);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;
	case RegNtDeleteValueKey:
	{
		PREG_DELETE_VALUE_KEY_INFORMATION pRegDelKeyInfo = (PREG_DELETE_VALUE_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegDeleteValueKey(NULL, pRegDelKeyInfo->Object, pRegDelKeyInfo->ValueName);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtSetInformationKey:
	{
		PREG_SET_INFORMATION_KEY_INFORMATION pRegOptInfo = (PREG_SET_INFORMATION_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegSetInformationKey(NULL, pRegOptInfo->Object, pRegOptInfo->KeySetInformationClass,
			pRegOptInfo->KeySetInformation, pRegOptInfo->KeySetInformationLength);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtRenameKey:
	{
		PREG_RENAME_KEY_INFORMATION pRegOptInfo = (PREG_RENAME_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegRenameKey(NULL, pRegOptInfo->Object, pRegOptInfo->NewName);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtEnumerateKey:
	{
		PREG_ENUMERATE_KEY_INFORMATION pRegOptInfo = (PREG_ENUMERATE_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegEnumerateKey(NULL, pRegOptInfo->Object, pRegOptInfo->Index,
			pRegOptInfo->KeyInformationClass, pRegOptInfo->Length);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtEnumerateValueKey:
	{
		PREG_ENUMERATE_VALUE_KEY_INFORMATION pRegOptInfo = (PREG_ENUMERATE_VALUE_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegEnumerateValueKey(NULL, pRegOptInfo->Object, pRegOptInfo->Index,
			pRegOptInfo->KeyValueInformationClass, pRegOptInfo->Length);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtQueryKey:
	{
		PREG_QUERY_KEY_INFORMATION pRegOptInfo = (PREG_QUERY_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegQueryKey(NULL, pRegOptInfo->Object, pRegOptInfo->KeyInformationClass,
			pRegOptInfo->Length);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtQueryValueKey:
	{
		PREG_QUERY_VALUE_KEY_INFORMATION pRegOptInfo = (PREG_QUERY_VALUE_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegQueryValueKey(NULL, pRegOptInfo->Object, pRegOptInfo->ValueName,
			pRegOptInfo->KeyValueInformationClass, pRegOptInfo->Length);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtQueryMultipleValueKey:
		NotifyType = NOTIFY_REG_QUERYMULTIPLEVALUEKEY;
		goto __label_reg_pre_common;

	case RegNtPostOpenKey:
	case RegNtPostDeleteKey:
	case RegNtPostSetValueKey:
	case RegNtPostDeleteValueKey:
	case RegNtPostSetInformationKey:
	case RegNtPostRenameKey:
	case RegNtPostQueryMultipleValueKey:
	case RegNtPostKeyHandleClose:
	case RegNtPostFlushKey:
	case RegNtPostLoadKey:
	case RegNtPostUnLoadKey:
	case RegNtPostQueryKeySecurity:
	case RegNtPostSetKeySecurity:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			if (pPostInfo->Seq != -1) {
				ProcmonGetPostLogEntry(pPostInfo->Seq, pPostOptInfo->Status, 0, &pLogBuffer);
				ProcmonNotifyProcessLog(pLogBuffer);
			}
			ProcmonFreePostInfo(pPostInfo);
		}
	}
	break;

	case RegNtKeyHandleClose:
		NotifyType = NOTIFY_REG_KEYHANDLECLOSE;
		goto __label_reg_pre_common;

	case RegNtPostEnumerateKey:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			if (pPostInfo->Seq != -1) {
				PREG_ENUMERATE_KEY_INFORMATION pRegOptInfo = (PREG_ENUMERATE_KEY_INFORMATION)pPostInfo->pRegData;
				ProcmonNotifyPostRegEnumerateKey(pPostInfo->Seq, pPostOptInfo->Status,
					pRegOptInfo->KeyInformationClass, pRegOptInfo->KeyInformation, pRegOptInfo->ResultLength);
				ProcmonFreePostInfo(pPostInfo);
			}
		}
	}
	break;
	case RegNtPostEnumerateValueKey:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			if (pPostInfo->Seq != -1) {
				PREG_ENUMERATE_VALUE_KEY_INFORMATION pRegOptInfo = (PREG_ENUMERATE_VALUE_KEY_INFORMATION)pPostInfo->pRegData;
				ProcmonNotifyPostRegEnumerateValueKey(pPostInfo->Seq, pPostOptInfo->Status,
					pRegOptInfo->KeyValueInformationClass, pRegOptInfo->KeyValueInformation, pRegOptInfo->ResultLength);
				ProcmonFreePostInfo(pPostInfo);
			}
		}
	}
	break;
	case RegNtPostQueryKey:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			if (pPostInfo->Seq != -1) {
				PREG_QUERY_KEY_INFORMATION pRegOptInfo = (PREG_QUERY_KEY_INFORMATION)pPostInfo->pRegData;
				ProcmonNotifyPostRegEnumerateKey(pPostInfo->Seq, pPostOptInfo->Status,
					pRegOptInfo->KeyInformationClass, pRegOptInfo->KeyInformation, pRegOptInfo->ResultLength);
				ProcmonFreePostInfo(pPostInfo);
			}
		}
	}
	break;

	case RegNtPostQueryValueKey:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			if (pPostInfo->Seq != -1) {
				PREG_QUERY_VALUE_KEY_INFORMATION pRegOptInfo = (PREG_QUERY_VALUE_KEY_INFORMATION)pPostInfo->pRegData;
				ProcmonNotifyPostRegEnumerateValueKey(pPostInfo->Seq, pPostOptInfo->Status,
					pRegOptInfo->KeyValueInformationClass, pRegOptInfo->KeyValueInformation, pRegOptInfo->ResultLength);
				ProcmonFreePostInfo(pPostInfo);
			}
		}
	}
	break;

	case RegNtPreCreateKeyEx:
	case RegNtPreOpenKeyEx:
	{
		PREG_CREATE_KEY_INFORMATION pRegOptInfo = (PREG_CREATE_KEY_INFORMATION)Argument2;
		ACCESS_MASK DesiredAccess;
		if (gBuildNumber <= 3790)
			DesiredAccess = 0;
		else
			DesiredAccess = pRegOptInfo->DesiredAccess;
		if (RegNotifyClass == RegNtPreOpenKeyEx){
			NotifyType = NOTIFY_REG_OPENKEYEX;
		}else{
			NotifyType = NOTIFY_REG_CREATEKEYEX;
		}
		Seq = ProcmonNotifyRegCreateOpenKeyEx(NotifyType, DesiredAccess, NULL, pRegOptInfo->RootObject, pRegOptInfo->CompleteName);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;

	case RegNtPostCreateKeyEx:
	case RegNtPostOpenKeyEx:
	{
		PREG_POST_OPERATION_INFORMATION pPostOptInfo = (PREG_POST_OPERATION_INFORMATION)Argument2;
		PREG_POST_INFO pPostInfo = ProcmonRegGetPrePostInfo();
		if (pPostInfo) {
			ULONG Disposition = 0;
			ACCESS_MASK DesiredAccess = 0, GrantedAccess = 0;
			PREG_CREATE_KEY_INFORMATION pRegOptInfo = (PREG_CREATE_KEY_INFORMATION)pPostInfo->pRegData;

			if (gBuildNumber > 3790) {
				if (pRegOptInfo->Disposition) {
					Disposition = *pRegOptInfo->Disposition;
				}
				GrantedAccess = pRegOptInfo->GrantedAccess;
				DesiredAccess = pRegOptInfo->DesiredAccess;
			}

			ProcmonNotifyPostRegCreateOpenKey(
				pPostInfo->Seq,
				pPostOptInfo->Status,
				DesiredAccess,
				GrantedAccess,
				Disposition);

			ProcmonFreePostInfo(pPostInfo);
		}
	}
	break;

	case RegNtPreFlushKey:
		NotifyType = NOTIFY_REG_FLUSHKEY;
		goto __label_reg_pre_common;

	case RegNtPreLoadKey:
	{
		PREG_LOAD_KEY_INFORMATION pRegOptInfo = (PREG_LOAD_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegLoadKey(pRegOptInfo->Object, pRegOptInfo->KeyName,
			pRegOptInfo->SourceFile);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;
	case RegNtPreUnLoadKey:
	{
		PREG_UNLOAD_KEY_INFORMATION pRegOptInfo = (PREG_UNLOAD_KEY_INFORMATION)Argument2;
		Seq = ProcmonNotifyRegUnLoadKey(NULL, pRegOptInfo->Object);
		ProcmonAddToPrePostList(Seq, Argument2);
	}
	break;
	case RegNtPreQueryKeySecurity:
		NotifyType = NOTIFY_REG_QUERYKEYSECURITY;
		goto __label_reg_pre_common;
	case RegNtPreSetKeySecurity:
		NotifyType = NOTIFY_REG_SETKEYSECURITY;
	__label_reg_pre_common:
		Seq = ProcmonNotifyRegOptCommon(NULL, *(PVOID*)Argument2, NotifyType, sizeof(USHORT));
		ProcmonAddToPrePostList(Seq, Argument2);
		break;
	default:
		break;
	}

	return STATUS_SUCCESS;
}

NTSTATUS
EnableRegMonitor(
	_In_ ULONG bEnable
)
{
	if (!bEnable) {

		if (gbRegCallbackSet && fnCmUnRegisterCallback)
		{
			CleanUpAllRegPostInfoList();
			fnCmUnRegisterCallback(gCookie);
			CleanupRegObjectList();
			gbRegCallbackSet = 0;
		}
		return STATUS_SUCCESS;
	}

	if (gbRegCallbackSet) {
		return STATUS_SUCCESS;
	}

	if ((fnCmRegisterCallback || fnCmRegisterCallbackEx) && !(bEnable & 8)) {
		if (fnCmRegisterCallbackEx) {
			UNICODE_STRING Altitude;
			RtlInitUnicodeString(&Altitude, L"500000");
			fnCmRegisterCallbackEx(ProcmonRegistryCallback, &Altitude, gDriverObject, NULL, &gCookie, NULL);
			gbRegCallbackSet = TRUE;
			return STATUS_SUCCESS;
		}
		fnCmRegisterCallback(ProcmonRegistryCallback, NULL, &gCookie);
	}

	gbRegCallbackSet = TRUE;
	return STATUS_SUCCESS;
}


VOID
ProcmonRegMonitorInit(
	VOID
)
{
	UNICODE_STRING UniStrFunction;

	RtlInitUnicodeString(&gUniStrInsufficentRef, L"<INSUFFICIENT RESOURCES>");
	RtlInitUnicodeString(&gUniInvalidName, L"<INVALID NAME>");
	RtlInitUnicodeString(&gUniStrDefault, L"(Default)");
	RtlInitUnicodeString(&gUniRegistry, L"\\Registry");
	if (gBuildNumber >= 3790)
	{
		RtlInitUnicodeString(&UniStrFunction, L"CmRegisterCallback");
		fnCmRegisterCallback = (FNCmRegisterCallback)MmGetSystemRoutineAddress(&UniStrFunction);
		RtlInitUnicodeString(&UniStrFunction, L"CmRegisterCallbackEx");
		fnCmRegisterCallbackEx = (FNCmRegisterCallbackEx)MmGetSystemRoutineAddress(&UniStrFunction);
		RtlInitUnicodeString(&UniStrFunction, L"CmUnRegisterCallback");
		fnCmUnRegisterCallback = (FNCmUnRegisterCallback)MmGetSystemRoutineAddress(&UniStrFunction);
		RtlInitUnicodeString(&UniStrFunction, L"CmCallbackGetKeyObjectID");
		fnCmCallbackGetKeyObjectID = (FNCmCallbackGetKeyObjectID)MmGetSystemRoutineAddress(&UniStrFunction);
		ExInitializePagedLookasideList(&gLookasideRegPostInfo, NULL, NULL, 0, sizeof(REG_POST_INFO), 'mgeR', 0x100);
		InitializeListHead(&gRegPostInfoList);
		KeInitializeMutex(&gMutexRegPostInfo, 0);
	}

	InitializeListHead(&gRegObjectList);
	KeInitializeMutex(&gMutexRegObjectList, 0);
}