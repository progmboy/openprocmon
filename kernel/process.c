
#include <ntifs.h>
#include <ntstrsafe.h>

#include "ntheader.h"
#include "globals.h"
#include "utils.h"
#include "log.h"
#include "process.h"
#include "file.h"

#define _countof(array) (sizeof(array)/sizeof(array[0]))

ULONG gProcessNameOff;
KTIMER gWaitProcessExitTimer;
KDPC gWaitProcessExitDpc;
FAST_MUTEX gProcessListMutex;
LIST_ENTRY gProcessInfoList;
LIST_ENTRY gProcessDelayExitList;
LONG gGlobalSeq = 0;
NPAGED_LOOKASIDE_LIST gLookasideListThreadProfiling;
KTIMER gTimerProcessProfiling;
KTIMER gTimerThreadProfiling;
KEVENT gEventProfilingReset;
LIST_ENTRY gListEntryArray[0x100];
BOOLEAN gThreadMonitorEnable = FALSE;
BOOLEAN gProcessMonitorEnable = FALSE;
BOOLEAN gSystemModuleLoadMonitorEnable = FALSE;
BOOLEAN gLoadImageMointorEnable = FALSE;
HANDLE ghThreadModuleMonitor;
KEVENT gModuleMonitorExitEvent;
HANDLE gSystemProcessId;
LIST_ENTRY gProcessWaitExitList;


FNZwQueryInformationThread fnZwQueryInformationThread;
FNSeLocateProcessImageName fnSeLocateProcessImageName;
FNPsSetCreateThreadNotifyRoutineEx fnPsSetCreateThreadNotifyRoutineEx;
FNPsSetCreateProcessNotifyRoutineEx2 fnPsSetCreateProcessNotifyRoutineEx2;
FNZwOpenProcessTokenEx fnZwOpenProcessTokenEx;


BOOLEAN
ProcmonQueryProcessInfoFromPeb(
	_In_ HANDLE hProcess,
	_In_ HANDLE ProcessId,
	_In_ PPEB Peb,
	_Out_ PUNICODE_STRING pUniStrProcessName,
	_Out_ PUNICODE_STRING pUniStrCommandLine,
	_Out_ PUNICODE_STRING pUniStrCurrentDirectory,
	_Out_ PVOID *ppEnvironment,
	_Out_ PULONG pEnvLength
)
/*++

Routine Description:

	.

Arguments:

	 -

Return Value:

	If get process name return TRUE. else FALSE

--*/
{
	NTSTATUS Status;
	KAPC_STATE ApcState;

	UNREFERENCED_PARAMETER(ProcessId);

	BOOLEAN bRet = (pUniStrProcessName == NULL);
	if (pUniStrCurrentDirectory) {
		pUniStrCurrentDirectory->Length = pUniStrCurrentDirectory->MaximumLength = 0;
		pUniStrCurrentDirectory->Buffer = NULL;
	}

	if (ppEnvironment) {
		*ppEnvironment = NULL;
	}

	if (pEnvLength) {
		*pEnvLength = 0;
	}

	if (hProcess && Peb) {
		PRTL_USER_PROCESS_PARAMETERS ProcessParameters;
		PEPROCESS Process;

		Status = ObReferenceObjectByHandle(hProcess, 0, NULL, 0, &Process, NULL);
		if (!NT_SUCCESS(Status))
			return FALSE;


		//
		// Try to Attach process
		//

		KeStackAttachProcess(Process, &ApcState);

		//
		// Peb is usermode address so we need try except
		//

		try {
			ProcessParameters = Peb->ProcessParameters;
			if (ProcessParameters) {
				UNICODE_STRING uniStrTemp;
				if (pUniStrProcessName) {

					try {
						if (ProcessParameters->Flags & 1) {
							uniStrTemp.Buffer = ProcessParameters->ImagePathName.Buffer;
						}
						else {
							uniStrTemp.Buffer = (PWCH)((ULONG_PTR)ProcessParameters + (ULONG_PTR)ProcessParameters->ImagePathName.Buffer);
						}

						uniStrTemp.Length = ProcessParameters->ImagePathName.Length;
						uniStrTemp.MaximumLength = ProcessParameters->ImagePathName.MaximumLength;

						ProcmonDuplicateUnicodeString2(pUniStrProcessName, &uniStrTemp, '5');
						bRet = pUniStrProcessName->Buffer != NULL;
					}
					except (EXCEPTION_EXECUTE_HANDLER) {
						if (pUniStrProcessName->Buffer) {
							ExFreePoolWithTag(pUniStrProcessName->Buffer, 0);
						}
					}

				}

				if (pUniStrCommandLine) {

					try {
						if (ProcessParameters->Flags & 1) {
							uniStrTemp.Buffer = ProcessParameters->CommandLine.Buffer;
						}
						else {
							uniStrTemp.Buffer = (PWCH)((ULONG_PTR)ProcessParameters + (ULONG_PTR)ProcessParameters->CommandLine.Buffer);
						}
						uniStrTemp.Length = ProcessParameters->CommandLine.Length;
						uniStrTemp.MaximumLength = ProcessParameters->CommandLine.MaximumLength;

						ProcmonDuplicateUnicodeString2(pUniStrCommandLine, &uniStrTemp, '6');
					}
					except (EXCEPTION_EXECUTE_HANDLER) {
						if (pUniStrCommandLine->Buffer) {
							ExFreePoolWithTag(pUniStrCommandLine->Buffer, 0);
						}
					}
				}

				if (pUniStrCurrentDirectory) {

					try {
						if (ProcessParameters->Flags & 1) {
							uniStrTemp.Buffer = ProcessParameters->CurrentDirectory.DosPath.Buffer;
						}
						else {
							uniStrTemp.Buffer = (PWCH)((ULONG_PTR)ProcessParameters +
								(ULONG_PTR)ProcessParameters->CurrentDirectory.DosPath.Buffer);
						}
						uniStrTemp.Length = ProcessParameters->CurrentDirectory.DosPath.Length;
						uniStrTemp.MaximumLength = ProcessParameters->CurrentDirectory.DosPath.MaximumLength;

						ProcmonDuplicateUnicodeString2(pUniStrCurrentDirectory, &uniStrTemp, '7');
					}
					except (EXCEPTION_EXECUTE_HANDLER) {
						if (pUniStrCurrentDirectory->Buffer) {
							ExFreePoolWithTag(pUniStrCurrentDirectory->Buffer, 0);
						}
					}

				}

				if (ppEnvironment && pEnvLength) {
					try {
						PVOID pEnvNew;
						ULONG EnvironmentLength;
						PWCHAR s = (PWCHAR)ProcessParameters->Environment;
						while (*s++) {
							while (*s++) {
							}
						}

						EnvironmentLength = (ULONG)(s - (PWCHAR)ProcessParameters->Environment) * sizeof(WCHAR);
						pEnvNew = ProcmonAllocatePoolWithTag(NonPagedPool, EnvironmentLength, '8');
						*ppEnvironment = pEnvNew;
						if (pEnvNew) {
							RtlCopyMemory(pEnvNew, ProcessParameters->Environment, EnvironmentLength);
							*pEnvLength = EnvironmentLength;
						}

					}
					except (EXCEPTION_EXECUTE_HANDLER) {
						if (ppEnvironment) {
							ExFreePoolWithTag(ppEnvironment, 0);
						}
					}
				}
			}
		}
		except (EXCEPTION_EXECUTE_HANDLER) {
			NOTHING;
		}

		KeUnstackDetachProcess(&ApcState);
		ObDereferenceObject(Process);
	}

	return bRet;

}

#define IDLE_NAME "Idle"

NTSTATUS
ProcmonQueryProcessNameFromProcessObject(
	_In_ HANDLE hProcess,
	_Out_ PUNICODE_STRING pUniStrProcessName
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
	NTSTATUS Status = STATUS_SUCCESS;
	PEPROCESS Process;
	ANSI_STRING StrProcName;
	CHAR ProcNameBuf[0x1F] = { 0 };

	pUniStrProcessName->Length = pUniStrProcessName->MaximumLength = 0;
	StrProcName.Buffer = ProcNameBuf;
	StrProcName.MaximumLength = sizeof(ProcNameBuf);

	if (!hProcess) {

		//
		// Idle
		//

		RtlStringCchCopyA(ProcNameBuf, sizeof(ProcNameBuf), "Idle");
		Status = STATUS_SUCCESS;
	}
	else {
		Status = ObReferenceObjectByHandle(hProcess, 0, NULL, KernelMode, &Process, NULL);
		if (NT_SUCCESS(Status)) {
			RtlStringCchCopyNA(ProcNameBuf, sizeof(ProcNameBuf), (PCHAR)((ULONG_PTR)Process + gProcessNameOff),
				sizeof(ProcNameBuf));
		}
	}

	if (ProcNameBuf[0]) {

		USHORT nNameLength, nUniNameLength;

		ProcNameBuf[sizeof(ProcNameBuf) - 1] = '\0';
		nNameLength = (USHORT)strlen(ProcNameBuf);

		StrProcName.Length = nNameLength;
		nUniNameLength = (nNameLength + 1) * sizeof(WCHAR);
		pUniStrProcessName->MaximumLength = nUniNameLength;
		pUniStrProcessName->Buffer = (PWCH)ProcmonAllocatePoolWithTag(NonPagedPool, nUniNameLength, 'G');
		if (pUniStrProcessName->Buffer) {
			Status = RtlAnsiStringToUnicodeString(pUniStrProcessName, &StrProcName, FALSE);
		}
	}

	return Status;
}


VOID
ProcmonFillProcessInfo(
	_In_ PPROCESSINFO_LIST pProcessInfo
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
	CLIENT_ID ClientId;
	OBJECT_ATTRIBUTES ObjectAttributes;
	HANDLE hProcess = NULL;
	PROCESS_EXTENDED_BASIC_INFORMATION ProcessExtBasicInfo = { 0 };
	PROCESS_SESSION_INFORMATION SessinInfo = {0};
	PVOID pProcBasicInfo;
	ULONG Length, LogBufLength = 0, LogBufExtLength = 0;
	HANDLE hToken;
	PTOKEN_USER pTokenUser = NULL;
	ULONG TokenUserLength = 0;
	TOKEN_STATISTICS Statistics;
	ULONG VirtualizationEnabled = 0;
	PTOKEN_MANDATORY_LABEL pIntegrityLevel = NULL;
	ULONG IntegrityLevelSidLength = 0;
	ULONG_PTR pWow64Information = 0;
	PLOG_BUFFER pLogBuf;
	PPROCESS_FULL_INFO pProcessFullInfo;
	UNICODE_STRING UniStrProcessName = {0};
	UNICODE_STRING UniStrCommandLine = { 0 };
	UNICODE_STRING UniStrCurrentDirectory = {0};
	PVOID pEnvironment = NULL;
	ULONG EnvLength = 0;
	KERNEL_USER_TIMES KernelUserTime;
	PLOG_PROCESSCREATE_INFO pProcCreateInfo;
 
	RtlZeroMemory(&Statistics, sizeof(Statistics));

	pProcessInfo->bInit = TRUE;

	if (pProcessInfo->ProcessId){

		//
		// try to open process
		//

		ClientId.UniqueThread = 0;
		ClientId.UniqueProcess = pProcessInfo->ProcessId;
		InitializeObjectAttributes(&ObjectAttributes, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);
		Status = ZwOpenProcess(&hProcess, 0, &ObjectAttributes, &ClientId);
		if (!NT_SUCCESS(Status)) {
			pProcessInfo->bInit = FALSE;
			return;
		}

		if (gBuildNumber < 6000) {

			//
			// Xp or lower
			//

			LARGE_INTEGER Timeout;

			Timeout.QuadPart = 0;
			if (ZwWaitForSingleObject(hProcess, 0, &Timeout) != STATUS_TIMEOUT) {
				ZwClose(hProcess);
				pProcessInfo->bInit = FALSE;
				return;
			}
		}

		if (gBuildNumber < 6000) {
			pProcBasicInfo = &ProcessExtBasicInfo.BasicInfo;
			Length = sizeof(PROCESS_BASIC_INFORMATION);
		}else{
			pProcBasicInfo = &ProcessExtBasicInfo;
			ProcessExtBasicInfo.Size = Length = sizeof(PROCESS_EXTENDED_BASIC_INFORMATION);

		}

		//
		// Query process basic information
		//

		Status = ZwQueryInformationProcess(hProcess, ProcessBasicInformation,
			pProcBasicInfo, Length, NULL);
		if (!NT_SUCCESS(Status)) {
			ZwClose(hProcess);
			pProcessInfo->bInit = FALSE;
			return;
		}

		//
		// Query process session information
		// Always success
		//

		ZwQueryInformationProcess(hProcess, ProcessSessionInformation, &SessinInfo,
			sizeof(SessinInfo), NULL);
		
		if (fnZwOpenProcessTokenEx)
			Status = (fnZwOpenProcessTokenEx)(hProcess, TOKEN_READ, OBJ_KERNEL_HANDLE, &hToken);
		else
			Status = ZwOpenProcessToken(hProcess, TOKEN_READ, &hToken);
		if (NT_SUCCESS(Status)) {
			pTokenUser = ProcmonQueryTokenInformation(hToken, &Statistics, &VirtualizationEnabled, &pIntegrityLevel);
			if (pTokenUser) {
				TokenUserLength = RtlLengthSid(pTokenUser->User.Sid);
			}

			if (pIntegrityLevel) {
				IntegrityLevelSidLength = RtlLengthSid(pIntegrityLevel->Label.Sid);
			}

			ZwClose(hToken);
		}

		Status = ZwQueryInformationProcess(hProcess, ProcessWow64Information,
			&pWow64Information, sizeof(pWow64Information), NULL);

		if (!NT_SUCCESS(Status)) {
			pWow64Information = 0;
		}
	}

	//
	// Exchange the process full info
	//

	pProcessFullInfo = (PPROCESS_FULL_INFO)InterlockedExchangePointer(&pProcessInfo->pProcessFullInfo, NULL);
	if (pProcessFullInfo) {
		if (pProcessFullInfo->ImageFileName.Length) {
			ProcmonDuplicateUnicodeString2(&UniStrProcessName, &pProcessFullInfo->ImageFileName, '5');
		}

		if (pProcessFullInfo->CommandLine.Length) {
			ProcmonDuplicateUnicodeString2(&UniStrCommandLine, &pProcessFullInfo->CommandLine, '6');
		}
	}

	if (/*ProcessExtBasicInfo.IsSubsystemProcess*/ProcessExtBasicInfo.Flags & 0x100) {

		//
		// Try to get subsystem process name
		//

		if (UniStrProcessName.Length == 0) {

			PEPROCESS pProcess;
			Status = ObReferenceObjectByHandle(hProcess, 0, NULL, KernelMode, &pProcess, NULL);
			if (NT_SUCCESS(Status)) {

				PUNICODE_STRING pUniStrProcessName;

				Status = fnSeLocateProcessImageName(pProcess, &pUniStrProcessName);
				if (NT_SUCCESS(Status) && pUniStrProcessName) {
					ProcmonDuplicateUnicodeString2(&UniStrProcessName, pUniStrProcessName, '5');

					//
					// clean up
					//

					ExFreePoolWithTag(pUniStrProcessName, 0);
				}
				ObDereferenceObject(pProcess);
			}
		}
	}else{

		BOOLEAN bRet;
		PVOID *ppEnvironment = &pEnvironment;
		PULONG pEnvLength = &EnvLength;
		PUNICODE_STRING pUniStrCurrentDir = &UniStrCurrentDirectory;
		PUNICODE_STRING pUniStrProcessName = &UniStrProcessName;
		PUNICODE_STRING pUniStrCommandLine = &UniStrCommandLine;

		if (!pProcessFullInfo) {
			ppEnvironment = NULL;
			pEnvLength = NULL;
			pUniStrCurrentDir = NULL;
		}

		if (UniStrCommandLine.Length) {
			pUniStrCommandLine = NULL;
		}

		if (UniStrProcessName.Length) {
			pUniStrProcessName = NULL;
		}

		bRet = ProcmonQueryProcessInfoFromPeb(hProcess, pProcessInfo->ProcessId,
			ProcessExtBasicInfo.BasicInfo.PebBaseAddress, pUniStrProcessName, pUniStrCommandLine,
			pUniStrCurrentDir, ppEnvironment, pEnvLength);
		if (!bRet) {

			//
			// Failed we need get the process name from process object
			//

			ProcmonQueryProcessNameFromProcessObject(hProcess, &UniStrProcessName);

		}
	}

	//
	// Get process time
	//

	if (pProcessFullInfo) {
		KernelUserTime.CreateTime = ProcmonGetTime();
	}
	else {
		KernelUserTime.CreateTime.QuadPart = 0;
		if (hProcess) {
			ZwQueryInformationProcess(hProcess, ProcessTimes, &KernelUserTime, sizeof(KernelUserTime), NULL);
			if (!KernelUserTime.CreateTime.QuadPart) {
				SYSTEM_TIMEOFDAY_INFORMATION SystemTime;
				if (NT_SUCCESS(ZwQuerySystemInformation(SystemTimeOfDayInformation, &SystemTime,
					sizeof(SystemTime), NULL))) {
					KernelUserTime.CreateTime.QuadPart = SystemTime.BootTime.QuadPart - SystemTime.BootTimeBias;
				}
			}
		}
	}

	if (hProcess) {
		ZwClose(hProcess);
	}

	if (UniStrProcessName.Length) {
		LogBufLength = UniStrProcessName.Length;
	}

	if (UniStrCommandLine.Length) {
		LogBufLength += UniStrCommandLine.Length;
	}

	if (pTokenUser) {
		LogBufLength += (UCHAR)TokenUserLength;
	}

	if (pIntegrityLevel) {
		LogBufLength += (UCHAR)IntegrityLevelSidLength;
	}

	if (pProcessFullInfo) {
		LONG Seq;
		if (pProcessFullInfo->pParentProcessInfo) {
			Seq = pProcessFullInfo->pParentProcessInfo->Seq;
		}else{
			Seq = -1;
		}

		pProcCreateInfo = (PLOG_PROCESSCREATE_INFO)ProcmonGetLogEntryAndInit(
			MONITOR_TYPE_PROCESS, NOTIFY_PROCESS_CREATE, Seq, STATUS_SUCCESS,
			LogBufLength + sizeof(LOG_PROCESSCREATE_INFO), &pLogBuf,
			pProcessFullInfo->StackFrameCounts, pProcessFullInfo->StackFrame);
		DerefProcessInfo(pProcessFullInfo->pParentProcessInfo);
	}else{
		pProcCreateInfo = (PLOG_PROCESSCREATE_INFO)ProcmonGetLogEntryAndInit(
			MONITOR_TYPE_PROCESS, NOTIFY_PROCESS_INIT, pProcessInfo->Seq, STATUS_SUCCESS,
			LogBufLength + sizeof(LOG_PROCESSCREATE_INFO),
			&pLogBuf, 0, NULL);
	}

	if (pProcCreateInfo) {
		PPROCESSINFO_LIST pParentProcInfo;
		ULONG_PTR pBufferEnd = (ULONG_PTR)(pProcCreateInfo + 1);

		pProcCreateInfo->Seq = pProcessInfo->Seq;
		pProcCreateInfo->ParentId = (ULONG)ProcessExtBasicInfo.BasicInfo.InheritedFromUniqueProcessId;
		pParentProcInfo = RefProcessInfo((HANDLE)ProcessExtBasicInfo.BasicInfo.InheritedFromUniqueProcessId, FALSE);
		pProcCreateInfo->ParentProcSeq = pParentProcInfo ? pParentProcInfo->Seq : -1;
		DerefProcessInfo(pParentProcInfo);
		pProcCreateInfo->CreateTime = KernelUserTime.CreateTime;
		pProcCreateInfo->ProcessId = (ULONG)(ULONG_PTR)pProcessInfo->ProcessId;
		pProcCreateInfo->SessionId = SessinInfo.SessionId;
		pProcCreateInfo->TokenVirtualizationEnabled = VirtualizationEnabled;
		pProcCreateInfo->SidLength = (UCHAR)TokenUserLength;
		pProcCreateInfo->IntegrityLevelSidLength = (UCHAR)IntegrityLevelSidLength;
		pProcCreateInfo->IsWow64 = pWow64Information == 0;
		pProcCreateInfo->AuthenticationId = Statistics.AuthenticationId;
		pProcCreateInfo->CommandLineLength = UniStrCommandLine.Length >> 1;
		pProcCreateInfo->ProcNameLength = UniStrProcessName.Length >> 1;

		if (TokenUserLength) {
			RtlCopyMemory((PVOID)pBufferEnd, pTokenUser->User.Sid, TokenUserLength);
			pBufferEnd += TokenUserLength;
		}

		if (pIntegrityLevel) {
			RtlCopyMemory((PVOID)pBufferEnd, pIntegrityLevel->Label.Sid, IntegrityLevelSidLength);
			pBufferEnd += IntegrityLevelSidLength;
		}

		if (UniStrProcessName.Buffer) {
			RtlCopyMemory((PVOID)pBufferEnd, UniStrProcessName.Buffer, UniStrProcessName.Length);
			pBufferEnd += UniStrProcessName.Length;
		}

		if (UniStrCommandLine.Buffer) {
			RtlCopyMemory((PVOID)pBufferEnd, UniStrCommandLine.Buffer, UniStrCommandLine.Length);
		}

		ProcmonNotifyProcessLog(pLogBuf);
	}else{
		pProcessInfo->bInit = FALSE;
	}

	if (UniStrCommandLine.Length) {
		LogBufExtLength = UniStrCommandLine.Length;
	}

	if (UniStrCurrentDirectory.Length) {
		LogBufExtLength += UniStrCurrentDirectory.Length;
	}

	if (EnvLength) {
		LogBufExtLength += EnvLength;
	}

	if (pProcessFullInfo) {
		PLOG_PROCESSSTART_INFO pLogStartBuf = (PLOG_PROCESSSTART_INFO)ProcmonGetLogEntryAndCopyFrameChain(
			MONITOR_TYPE_PROCESS, NOTIFY_PROCESS_START, pProcessInfo->Seq, 0,
			LogBufExtLength + sizeof(LOG_PROCESSSTART_INFO), &pLogBuf);
		ULONG_PTR pBufEnd = (ULONG_PTR)(pLogStartBuf + 1);

		if (pLogStartBuf) {
			pLogStartBuf->ParentId = (ULONG)(ULONG_PTR)ProcessExtBasicInfo.BasicInfo.InheritedFromUniqueProcessId;
			pLogStartBuf->CommandLineLength = UniStrCommandLine.Length >> 1;
			pLogStartBuf->CurrentDirectoryLength = UniStrCurrentDirectory.Length >> 1;
			pLogStartBuf->EnvironmentLength = EnvLength >> 1;

			if (UniStrCommandLine.Buffer) {
				RtlCopyMemory((PVOID)pBufEnd, UniStrCommandLine.Buffer, UniStrCommandLine.Length);
				pBufEnd += UniStrCommandLine.Length;
			}

			if (UniStrCurrentDirectory.Buffer) {
				RtlCopyMemory((PVOID)pBufEnd, UniStrCurrentDirectory.Buffer, UniStrCurrentDirectory.Length);
				pBufEnd += UniStrCurrentDirectory.Length;
			}

			if (EnvLength) {
				RtlCopyMemory((PVOID)pBufEnd, pEnvironment, EnvLength);
			}

			ProcmonNotifyProcessLog(pLogBuf);
		}
	}

	//
	// Clean up
	//

	if (pProcessFullInfo)
		ExFreePoolWithTag(pProcessFullInfo, 0);
	if (TokenUserLength)
		ExFreePoolWithTag(pTokenUser, 0);
	if (UniStrProcessName.Length)
		ExFreePoolWithTag(UniStrProcessName.Buffer, 0);
	if (UniStrCommandLine.Length)
		ExFreePoolWithTag(UniStrCommandLine.Buffer, 0);
	if (EnvLength)
		ExFreePoolWithTag(pEnvironment, 0);
	if (UniStrCurrentDirectory.Length)
		ExFreePoolWithTag(UniStrCurrentDirectory.Buffer, 0);
	if (IntegrityLevelSidLength)
		ExFreePoolWithTag(pIntegrityLevel, 0);

}

PPROCESSINFO_LIST
FindProcessInfoByProcessId(
	_In_ HANDLE ProcessId
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
	PLIST_ENTRY pEntry;

	if (IsListEmpty(&gProcessInfoList)) {
		return NULL;
	}

	for (pEntry = gProcessInfoList.Flink;
		pEntry != &gProcessInfoList;
		pEntry = pEntry->Flink)
	{
		PPROCESSINFO_LIST pProcessInfo = CONTAINING_RECORD(pEntry, PROCESSINFO_LIST, List);
		if (pProcessInfo->ProcessId == ProcessId) {

			//
			// find it, add refcount.
			//

			InterlockedIncrement(&pProcessInfo->RefCount);

			//
			// if this Entry not at the list head.move to list head
			//

			if (gProcessInfoList.Flink != pEntry) {
				RemoveEntryList(pEntry);
				InsertHeadList(&gProcessInfoList, pEntry);
			}

			return pProcessInfo;
		}
	}

	return NULL;
}

PPROCESSINFO_LIST
RefProcessInfo(
	_In_ HANDLE ProcessId,
	_In_ BOOLEAN bNotAsyn
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
	PPROCESSINFO_LIST pProcessInfoFind;
	PPROCESSINFO_LIST pNewProcessInfo;

	//
	// 这里查询也需要锁得原因是由于在查询的过程中
	// 如果查询到ProcessInfo我们会将ProcessInfo放在链表的头部
	// 这样就对链表进行了修改,所以这里需要加锁
	//

	ExAcquireFastMutex(&gProcessListMutex);
	if (!gFlags) {
		ExReleaseFastMutex(&gProcessListMutex);
		return NULL;
	}

	pProcessInfoFind = FindProcessInfoByProcessId(ProcessId);
	ExReleaseFastMutex(&gProcessListMutex);
	if (pProcessInfoFind == NULL) {

		//
		// This process do not in process list
		// allocate a new one and add to process list
		//

		pNewProcessInfo = (PPROCESSINFO_LIST)ProcmonAllocatePoolWithTag(NonPagedPool,
			sizeof(PROCESSINFO_LIST), '7');
		if (pNewProcessInfo) {
			pNewProcessInfo->ProcessId = ProcessId;
			pNewProcessInfo->RefCount = 1;
			pNewProcessInfo->Seq = InterlockedExchangeAdd(&gGlobalSeq, 1) + 1;
			pNewProcessInfo->bInit = FALSE;
			pNewProcessInfo->pProcessFullInfo = NULL;
			pNewProcessInfo->Process = NULL;

			//
			// Find process again. because ProcmonAllocatePoolWithTag is slow
			// Another thread may add to list already.
			// 作者的意思:
			// 这里再次在链表里查找, 因为ProcmonAllocatePoolWithTag可能会慢.所以其他线程
			// 很有可能已经创建了这个Process结构, 所以我们再查询一下.
			//
			// 个人感觉：
			// 其实没这比较就直接一个锁锁住,然后添加就完事了,性能应该不会有太大的降低
			//

			ExAcquireFastMutex(&gProcessListMutex);
			pProcessInfoFind = FindProcessInfoByProcessId(ProcessId);

			//
			// 如果没有找到, 并且监控开关已经打开
			//

			if (!pProcessInfoFind && gFlags) {

				//
				// 添加到链表中去
				//

				InsertHeadList(&gProcessInfoList, &pNewProcessInfo->List);
				pProcessInfoFind = pNewProcessInfo;
				InterlockedIncrement(&pNewProcessInfo->RefCount);
			}
			ExReleaseFastMutex(&gProcessListMutex);

			//
			// 如果是新添加的
			//

			if (pProcessInfoFind == pNewProcessInfo) {

				//
				// 如果是同步IO,则立即填充ProcessInfo
				//

				if (bNotAsyn) {
					ProcmonFillProcessInfo(pProcessInfoFind);
				}
			}else{

				//
				// 这里表示其他线程已经添加了ProcessInfo
				// 不用我们再次添加,释放掉
				//

				ExFreePoolWithTag(pNewProcessInfo, 0);
			}
		}
	}

	if (pProcessInfoFind) {
		if (pProcessInfoFind->pProcessFullInfo || (bNotAsyn && !pProcessInfoFind->bInit)) {
			ProcmonFillProcessInfo(pProcessInfoFind);
		}
	}

	return pProcessInfoFind;
}

VOID
DerefProcessInfo(
	_In_ PPROCESSINFO_LIST pProcessInfo
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
	BOOLEAN bNeedFree = FALSE;

	if (!pProcessInfo) {
		return;
	}

	if (!InterlockedDecrement(&pProcessInfo->RefCount)) {

		ExAcquireFastMutex(&gProcessListMutex);
		if (!pProcessInfo->RefCount) {
			if (pProcessInfo->List.Flink){
				RemoveEntryList(&pProcessInfo->List);
			}
			bNeedFree = TRUE;
		}
		ExReleaseFastMutex(&gProcessListMutex);
		if (bNeedFree) {
			ExFreePoolWithTag(pProcessInfo, 0);
		}
	}
}

BOOLEAN
RefThreadInfo(
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
	PETHREAD Thread;
	PLIST_ENTRY pEntry;
	PTHREADINFO_LIST pThreadInfo;
	BOOLEAN bFind = FALSE;

	//
	// Lock
	//

	Thread = KeGetCurrentThread();
	ExAcquireFastMutex(&gThreadInfoMutex);

	for (pEntry = gThreadInfoList.Flink;
		pEntry != &gThreadInfoList;
		pEntry = pEntry->Flink) {
		pThreadInfo = CONTAINING_RECORD(pEntry, THREADINFO_LIST, List);
		if (pThreadInfo->Thread == Thread) {

			//
			//  This thread has been record
			//

			++pThreadInfo->RefCount;
			//InterlockedIncrement(&pThreadInfo->RefCount);
			bFind = TRUE;
		}
	}

	if (!bFind) {

		//
		// Not find in thread list. add a new record
		//

		pThreadInfo = ExAllocateFromNPagedLookasideList(&gNPagedLooksideListThreadInfo);
		pThreadInfo->RefCount = 1;
		pThreadInfo->Thread = Thread;
		InsertHeadList(&gThreadInfoList, &pThreadInfo->List);
	}

	ExReleaseFastMutex(&gThreadInfoMutex);
	return bFind;

}

VOID
DeRefThreadInfo(
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
	PETHREAD Thread;
	PLIST_ENTRY pEntry;
	PTHREADINFO_LIST pThreadInfo = NULL;

	//
	// Lock
	//

	Thread = KeGetCurrentThread();
	ExAcquireFastMutex(&gThreadInfoMutex);

	for (pEntry = gThreadInfoList.Flink;
		pEntry != &gThreadInfoList;
		pEntry = pEntry->Flink) {
		PTHREADINFO_LIST pThreadInfoTmp = CONTAINING_RECORD(pEntry, THREADINFO_LIST, List);
		if (pThreadInfoTmp->Thread == Thread) {
			pThreadInfo = pThreadInfoTmp;
			break;
		}
	}

	if (pThreadInfo){
		if (pThreadInfo->RefCount-- == 1) {
			
			//
			// remove from list
			//
			
			RemoveEntryList(pEntry);
			
			//
			// free the buffer
			//
			
			ExFreeToNPagedLookasideList(&gNPagedLooksideListThreadInfo, pThreadInfo);
		}
	}


	ExReleaseFastMutex(&gThreadInfoMutex);
}

VOID
ProcmonWaitProcessExitWorkRoutine(
	_In_ PVOID pWorkItem
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
	PLIST_ENTRY pEntry;
	PEPROCESS Process;
	PLIST_ENTRY pEntryTemp;
	PPROCESSINFO_LIST pProcessInfo;
	LARGE_INTEGER Timeout = { 0 };

	ExAcquireFastMutex(&gProcessListMutex);
	pEntry = gProcessWaitExitList.Flink;
	if (!IsListEmpty(&gProcessWaitExitList))
	{
		do
		{
			pProcessInfo = CONTAINING_RECORD(pEntry, PROCESSINFO_LIST, ProcessExitList);
			Process = pProcessInfo->Process;
			pEntryTemp = pEntry->Flink;

			//
			// Wait process exit 
			//

			if (KeWaitForSingleObject(Process, 0, 0, 0, &Timeout) != STATUS_TIMEOUT)
			{
				RemoveEntryList(&pProcessInfo->ProcessExitList);
				pProcessInfo->Process = NULL;
				if (!InterlockedDecrement(&pProcessInfo->RefCount) && !pProcessInfo->RefCount) {
					if (pProcessInfo->List.Flink) {
						RemoveEntryList(&pProcessInfo->List);
					}
					ExFreePoolWithTag(pProcessInfo, 0);
				}
				ObDereferenceObject(Process);
			}
			pEntry = pEntryTemp;
		} while (pEntryTemp != &gProcessWaitExitList);
		pEntry = gProcessWaitExitList.Flink;
	}
	if (pEntry != &gProcessWaitExitList) {
		LARGE_INTEGER TimerDue;
		TimerDue.QuadPart = -5000000;
		KeSetTimer(&gWaitProcessExitTimer, TimerDue, &gWaitProcessExitDpc);
	}
	ExReleaseFastMutex(&gProcessListMutex);
	ExFreePoolWithTag(pWorkItem, 0);
}

VOID
ProcmonWaitProcessExitDpcRoutine(
	_In_     struct _KDPC *Dpc,
	_In_opt_ PVOID        DeferredContext,
	_In_opt_ PVOID        SystemArgument1,
	_In_opt_ PVOID        SystemArgument2
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
	PWORK_QUEUE_ITEM pWorkItem;

	UNREFERENCED_PARAMETER(Dpc);
	UNREFERENCED_PARAMETER(DeferredContext);
	UNREFERENCED_PARAMETER(SystemArgument1);
	UNREFERENCED_PARAMETER(SystemArgument2);

	pWorkItem = ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(WORK_QUEUE_ITEM), 'M');
	if (pWorkItem) {
		pWorkItem->Parameter = pWorkItem;
		pWorkItem->WorkerRoutine = ProcmonWaitProcessExitWorkRoutine;
		pWorkItem->List.Flink = NULL;
		ExQueueWorkItem(pWorkItem, DelayedWorkQueue);
	}
}

VOID
ProcmonProcessMonitorInit(
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
	UNICODE_STRING UniStrFunctionName;
	PEPROCESS Process = IoGetCurrentProcess();

	for (ULONG Offset = 0; Offset < 0x3000; Offset++)
	{
		if (0 == strncmp("System", (const char*)((ULONG_PTR)Process + Offset), 6)) {
			gProcessNameOff = Offset;
			break;
		}
	}

	ExInitializeNPagedLookasideList(&gLookasideListThreadProfiling, NULL, NULL, 0, sizeof(THREAD_PROFILING_UPDATE_APC), 'nmP', 0);
	InitializeListHead(&gProcessInfoList);
	InitializeListHead(&gProcessWaitExitList);
	ExInitializeFastMutex(&gProcessListMutex);

	RtlInitUnicodeString(&UniStrFunctionName, L"ZwQueryInformationThread");
	fnZwQueryInformationThread = (FNZwQueryInformationThread)MmGetSystemRoutineAddress(&UniStrFunctionName);
	RtlInitUnicodeString(&UniStrFunctionName, L"SeLocateProcessImageName");
	fnSeLocateProcessImageName = (FNSeLocateProcessImageName)MmGetSystemRoutineAddress(&UniStrFunctionName);
	RtlInitUnicodeString(&UniStrFunctionName, L"PsSetCreateProcessNotifyRoutineEx2");
	fnPsSetCreateProcessNotifyRoutineEx2 = (FNPsSetCreateProcessNotifyRoutineEx2)MmGetSystemRoutineAddress(&UniStrFunctionName);
	RtlInitUnicodeString(&UniStrFunctionName, L"PsSetCreateThreadNotifyRoutineEx");
	fnPsSetCreateThreadNotifyRoutineEx = (FNPsSetCreateThreadNotifyRoutineEx)MmGetSystemRoutineAddress(&UniStrFunctionName);
	if (gBuildNumber >= 3790){
		RtlInitUnicodeString(&UniStrFunctionName, L"ZwOpenProcessTokenEx");
		fnZwOpenProcessTokenEx = (FNZwOpenProcessTokenEx)MmGetSystemRoutineAddress(&UniStrFunctionName);
	}

	KeInitializeTimer(&gWaitProcessExitTimer);
	KeInitializeDpc(&gWaitProcessExitDpc, ProcmonWaitProcessExitDpcRoutine, NULL);
	KeInitializeEvent(&gModuleMonitorExitEvent, SynchronizationEvent, 0);
	KeInitializeEvent(&gEventProfilingReset, SynchronizationEvent, 0);
	KeInitializeTimerEx(&gTimerThreadProfiling, SynchronizationTimer);
	KeInitializeTimerEx(&gTimerProcessProfiling, SynchronizationTimer);

	for (int i = 0; i < _countof(gListEntryArray); i++)
	{
		InitializeListHead(&gListEntryArray[i]);
	}

	gSystemProcessId = PsGetCurrentProcessId();

}


VOID
FreeAllProcessInfo(
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
	PLIST_ENTRY pEntry;
	ExAcquireFastMutex(&gProcessListMutex);

	pEntry = gProcessInfoList.Flink;
	if (!IsListEmpty(&gProcessInfoList)) {

		do
		{
			PLIST_ENTRY pEntryTmp = pEntry->Flink;
			PPROCESSINFO_LIST pProcList = CONTAINING_RECORD(pEntry, PROCESSINFO_LIST, List);
			if (pProcList->Process) {
				ObDereferenceObject(pProcList->Process);
				RemoveEntryList(&pProcList->ProcessExitList);
			}

			RemoveEntryList(pEntry);
			pEntry->Flink = NULL;

			if (!InterlockedDecrement(&pProcList->RefCount) && !pProcList->RefCount) {
				if (pEntry->Flink) {
					RemoveEntryList(pEntry);
				}
				ExFreePoolWithTag(pProcList, 0);
			}

			pEntry = pEntryTmp;
		} while (pEntry != &gProcessInfoList);

	}

	ExReleaseFastMutex(&gProcessListMutex);
}

NTSTATUS
ProcmonQueryThreadExitInfo(
	_In_ HANDLE ProcessId,
	_In_ HANDLE ThreadId,
	_Out_ PKERNEL_USER_TIMES pKernelUserTime
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
	HANDLE hThread;
	NTSTATUS Status = STATUS_SUCCESS;;
	OBJECT_ATTRIBUTES ObjectAttributes;
	CLIENT_ID ClientId;

	RtlZeroMemory(pKernelUserTime, sizeof(KERNEL_USER_TIMES));
	InitializeObjectAttributes(&ObjectAttributes, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);
	ClientId.UniqueProcess = ProcessId;
	ClientId.UniqueThread = ThreadId;

	if (NT_SUCCESS(ZwOpenThread(&hThread, 0, &ObjectAttributes, &ClientId)) && hThread) {

		THREAD_BASIC_INFORMATION ThreadBasicInfo;
		if (NT_SUCCESS(ZwQueryInformationThread(hThread, ThreadBasicInformation, &ThreadBasicInfo,
			sizeof(ThreadBasicInfo), NULL))) {
			Status = ThreadBasicInfo.ExitStatus;
		}
		ZwQueryInformationThread(hThread, ThreadTimes, pKernelUserTime, sizeof(*pKernelUserTime), NULL);
		ZwClose(hThread);
	}

	return Status;
}

VOID
CreateThreadNotifyRoutine(
	_In_ HANDLE ProcessId,
	_In_ HANDLE ThreadId,
	_In_ BOOLEAN Create
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
	NTSTATUS ThreadExitStatus;
	PLOG_BUFFER pLogBuf;
	if (gFlags & 1) {
		PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(ProcessId, TRUE);
		if (pProcessInfo) {
			if (Create) {
				PULONG pThreadId = (PULONG)ProcmonGetLogEntryAndCopyFrameChain(
					MONITOR_TYPE_PROCESS, 
					NOTIFY_THREAD_CREATE, 
					pProcessInfo->Seq, 0,
					sizeof(ULONG), &pLogBuf);
				if (pThreadId) {
					*pThreadId = (ULONG)(ULONG_PTR)ThreadId;
					ProcmonNotifyProcessLog(pLogBuf);
				}
			}else{
				KERNEL_USER_TIMES KernelUserTimes;
				ThreadExitStatus = ProcmonQueryThreadExitInfo(ProcessId, ThreadId, &KernelUserTimes);

				PLOG_THREADEXIT_INFO pLogThreadExitInfo = (PLOG_THREADEXIT_INFO)ProcmonGetLogEntryAndCopyFrameChain(
					MONITOR_TYPE_PROCESS, NOTIFY_THREAD_EXIT, pProcessInfo->Seq, 0,
					sizeof(LOG_THREADEXIT_INFO), &pLogBuf);
				if (pLogThreadExitInfo) {
					pLogThreadExitInfo->ExitStatus = ThreadExitStatus;
					pLogThreadExitInfo->KenrnelTime = KernelUserTimes.KernelTime;
					pLogThreadExitInfo->UserTime = KernelUserTimes.UserTime;

					ProcmonNotifyProcessLog(pLogBuf);
				}
			}
			DerefProcessInfo(pProcessInfo);
		}
	}
}

NTSTATUS
ProcmonQueryProcessExitInfo(
	_In_ BOOLEAN bRefProcess,
	_In_ PPROCESSINFO_LIST ProcessInfo,
	_Out_ PKERNEL_USER_TIMES pUserTime,
	_Out_ PVM_COUNTERS pVmCounters
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
	NTSTATUS ExitStatus = STATUS_SUCCESS;
	CLIENT_ID ClientId;
	OBJECT_ATTRIBUTES ObjectAttributes;
	PROCESS_BASIC_INFORMATION ProcessBasicInfo;
	HANDLE hProcess;

	RtlZeroMemory(pUserTime, sizeof(*pUserTime));
	RtlZeroMemory(pVmCounters, sizeof(*pVmCounters));

	ClientId.UniqueProcess = ProcessInfo->ProcessId;
	ClientId.UniqueThread = NULL;
	InitializeObjectAttributes(&ObjectAttributes, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);

	if (!NT_SUCCESS(ZwOpenProcess(&hProcess, 0, &ObjectAttributes, &ClientId)) || !hProcess)
		return STATUS_SUCCESS;

	if (!hProcess){
		return STATUS_SUCCESS;
	}

	if (NT_SUCCESS(ZwQueryInformationProcess(hProcess, ProcessBasicInformation, &ProcessBasicInfo,
		sizeof(PROCESS_BASIC_INFORMATION), NULL)))
		ExitStatus = ProcessBasicInfo.ExitStatus;
	ZwQueryInformationProcess(hProcess, ProcessTimes, pUserTime, sizeof(KERNEL_USER_TIMES), NULL);
	ZwQueryInformationProcess(hProcess, ProcessVmCounters, pVmCounters, sizeof(VM_COUNTERS), NULL);
	if (bRefProcess)
		ObReferenceObjectByHandle(hProcess, 0, NULL, 0, &ProcessInfo->Process, NULL);
	ZwClose(hProcess);
	return ExitStatus;
}


VOID
ProcmonAddProcessToWaitExitList(
	_In_  PPROCESSINFO_LIST pProcessInfo
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
	PWORK_QUEUE_ITEM pWorkItem;

	ExAcquireFastMutex(&gProcessListMutex);
	InsertHeadList(&gProcessWaitExitList, &pProcessInfo->ProcessExitList);
	ExReleaseFastMutex(&gProcessListMutex);
	pWorkItem = ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(WORK_QUEUE_ITEM), 'M');
	if (pWorkItem) {
		pWorkItem->Parameter = pWorkItem;
		pWorkItem->WorkerRoutine = ProcmonWaitProcessExitWorkRoutine;
		pWorkItem->List.Flink = NULL;
		ExQueueWorkItem(pWorkItem, DelayedWorkQueue);
	}
}

VOID
CreateProcessNotifyRoutineCommon(
	_In_ HANDLE ParentId,
	_In_ HANDLE ProcessId,
	_In_ BOOLEAN bCreate,
	_In_ PPS_CREATE_NOTIFY_INFO CreateInfo
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
	USHORT CommandLineLength;
	USHORT ImgFileNameLength;
	PPROCESSINFO_LIST pParentProcessInfo;
	PPROCESSINFO_LIST pProcessInfo;
	PPROCESS_FULL_INFO pNewFullInfo;
	PWCHAR pCommandBuffer;
	PPROCESSINFO_LIST pProcessInfoTmp;
	NTSTATUS ExitStatus;

	CommandLineLength = 0;
	ImgFileNameLength = 0;
	if (bCreate) {
		pParentProcessInfo = RefProcessInfo(ParentId, TRUE);
		pProcessInfo = RefProcessInfo(ProcessId, FALSE);
		if (pProcessInfo) {
			if (CreateInfo) {
				ImgFileNameLength = CreateInfo->ImageFileName ? CreateInfo->ImageFileName->Length : 0;
				CommandLineLength = CreateInfo->CommandLine ? CreateInfo->CommandLine->Length : 0;
			}
			pNewFullInfo = ProcmonAllocatePoolWithTag(NonPagedPool, CommandLineLength + ImgFileNameLength + sizeof(PROCESS_FULL_INFO), '8');
			if (pNewFullInfo) {
				USHORT StackFrameCounts;

				pNewFullInfo->pParentProcessInfo = pParentProcessInfo;
				StackFrameCounts = (USHORT)ProcmonGenStackFrameChain(TRUE, pNewFullInfo->StackFrame, MAX_STACKFRAME_COUNTS);
				pNewFullInfo->ImageFileName.Buffer = (PWCH)&pNewFullInfo[1];
				pNewFullInfo->ImageFileName.MaximumLength = ImgFileNameLength;
				pNewFullInfo->ImageFileName.Length = ImgFileNameLength;
				pNewFullInfo->StackFrameCounts = StackFrameCounts;
				if (ImgFileNameLength)
					RtlCopyMemory(&pNewFullInfo[1], CreateInfo->ImageFileName->Buffer, ImgFileNameLength);
				pNewFullInfo->CommandLine.MaximumLength = CommandLineLength;
				pNewFullInfo->CommandLine.Length = CommandLineLength;
				pCommandBuffer = (PWCHAR)((ULONG_PTR)pNewFullInfo->ImageFileName.Buffer + ImgFileNameLength);
				pNewFullInfo->CommandLine.Buffer = pCommandBuffer;
				if (CommandLineLength)
					RtlCopyMemory(pCommandBuffer, CreateInfo->CommandLine->Buffer, CommandLineLength);
				pProcessInfo->pProcessFullInfo = pNewFullInfo;
			}
			pProcessInfoTmp = pProcessInfo;
		}else{
			pProcessInfoTmp = pParentProcessInfo;
		}
		DerefProcessInfo(pProcessInfoTmp);
	}else {
		pProcessInfo = RefProcessInfo(ProcessId, TRUE);
		if (pProcessInfo) {
			KERNEL_USER_TIMES KernelUserTime;
			VM_COUNTERS VmCounters;
			PLOG_PROCESSBASIC_INFO pLogProcessBasicInfo;
			PLOG_BUFFER pLogBuf;

			ExitStatus = ProcmonQueryProcessExitInfo(TRUE, pProcessInfo, &KernelUserTime, &VmCounters);
			pLogProcessBasicInfo = ProcmonGetLogEntryAndCopyFrameChain(MONITOR_TYPE_PROCESS, NOTIFY_PROCESS_EXIT,
				pProcessInfo->Seq, 0, sizeof(LOG_PROCESSBASIC_INFO), &pLogBuf);
			if (pLogProcessBasicInfo){
				pLogProcessBasicInfo->ExitStatus = ExitStatus;
				pLogProcessBasicInfo->KenrnelTime.QuadPart = KernelUserTime.KernelTime.QuadPart;
				pLogProcessBasicInfo->UserTime.QuadPart = KernelUserTime.UserTime.QuadPart;
				pLogProcessBasicInfo->PagefileUsage = VmCounters.PagefileUsage;
				pLogProcessBasicInfo->PeakPagefileUsage = VmCounters.PeakPagefileUsage;
				pLogProcessBasicInfo->WorkingSetSize = VmCounters.WorkingSetSize;
				pLogProcessBasicInfo->PeakWorkingSetSize = VmCounters.PeakWorkingSetSize;
				ProcmonNotifyProcessLog(pLogBuf);
			}
			DerefProcessInfo(pProcessInfo);
			if (pProcessInfo->Process)
				ProcmonAddProcessToWaitExitList(pProcessInfo);
		}
	}
}

VOID
CreateProcessNotifyRoutineEx2(
	_In_ HANDLE Process,
	_In_ HANDLE ProcessId,
	_In_ PPS_CREATE_NOTIFY_INFO CreateInfo
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
	UNREFERENCED_PARAMETER(Process);

	if (CreateInfo)
		CreateProcessNotifyRoutineCommon(CreateInfo->ParentProcessId, ProcessId, TRUE, CreateInfo);
	else
		CreateProcessNotifyRoutineCommon(NULL, ProcessId, FALSE, NULL);
}

VOID
CreateProcessNotifyRoutine(
	_In_ HANDLE ParentId,
	_In_ HANDLE ProcessId,
	_In_ BOOLEAN Create
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
	CreateProcessNotifyRoutineCommon(ParentId, ProcessId, Create, NULL);
}

VOID
ProcmonProcessProfilingNotify(
	_In_ PSYSTEM_PROCESS_INFORMATION pSystemProcessInfo
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
	PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(pSystemProcessInfo->UniqueProcessId, 1);
	if (pProcessInfo && !pProcessInfo->pProcessFullInfo)
	{
		PLOG_BUFFER pLogBuf;
		PLOG_PROCESS_PROFILING_INFO pProcessProfInfo = (PLOG_PROCESS_PROFILING_INFO)ProcmonGetLogEntryAndInit(
			MONITOR_TYPE_PROFILING, NOTIFY_PROCESS_PROFILING,
			pProcessInfo->Seq, 0, 0x20, &pLogBuf, 0, NULL);
		if (pProcessProfInfo)
		{
			pProcessProfInfo->UserTime = pSystemProcessInfo->UserTime;
			pProcessProfInfo->KernelTime = pSystemProcessInfo->KernelTime;
			pProcessProfInfo->WorkingSetSize = pSystemProcessInfo->WorkingSetSize;
			pProcessProfInfo->PagefileUsage = pSystemProcessInfo->PagefileUsage;
			ProcmonNotifyProcessLog(pLogBuf);
		}
		DerefProcessInfo(pProcessInfo);
	}
}

PTHREAD_PROFILING_INFO
ProcmonGetThreadInfoFromList(
	_In_ PCLIENT_ID pClientId
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
	UCHAR Index = (UCHAR)((ULONG)(ULONG_PTR)pClientId->UniqueThread >> 4);
	PLIST_ENTRY pListHead = &gListEntryArray[Index];
	PTHREAD_PROFILING_INFO pThreadProfInfoNew;

	if (!IsListEmpty(pListHead)) {

		PLIST_ENTRY pEntry;
		for (pEntry = pListHead->Flink;
			pEntry != pListHead;
			pEntry = pEntry->Flink)
		{
			PTHREAD_PROFILING_INFO pThreadProfInfo = CONTAINING_RECORD(pEntry, THREAD_PROFILING_INFO, List);
			if (pThreadProfInfo->ClientId.UniqueThread == pClientId->UniqueThread) {
				return pThreadProfInfo;
			}
		}
	}

	//
	// Here listhead is empty or can not find the threadid in list
	// we need allocate a new one
	//

	pThreadProfInfoNew = (PTHREAD_PROFILING_INFO)ProcmonAllocatePoolWithTag(PagedPool,
		sizeof(THREAD_PROFILING_INFO), 'N');
	RtlZeroMemory(pThreadProfInfoNew, sizeof(THREAD_PROFILING_INFO));
	pThreadProfInfoNew->ClientId = *pClientId;
	InitializeListHead(&pThreadProfInfoNew->List);
	InsertHeadList(pListHead, &pThreadProfInfoNew->List);
	return pThreadProfInfoNew;
}


VOID
ProcmonThreadProfileUpdateRoutine(
	IN struct _KAPC *Apc,
	IN OUT PKNORMAL_ROUTINE *NormalRoutine,
	IN OUT PVOID *NormalContext,
	IN OUT PVOID *SystemArgument1,
	IN OUT PVOID *SystemArgument2
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
	PPROCESSINFO_LIST pProcessInfo;
	PTHREAD_PROFILING_UPDATE_APC pThreadProfUpdateApc = (PTHREAD_PROFILING_UPDATE_APC)Apc;

	UNREFERENCED_PARAMETER(NormalContext);
	UNREFERENCED_PARAMETER(SystemArgument1);
	UNREFERENCED_PARAMETER(SystemArgument2);


	pProcessInfo = RefProcessInfo(pThreadProfUpdateApc->ProcessId, TRUE);
	if (pProcessInfo) {
		PLOG_BUFFER pLogBuf;
		PLOG_THREAD_PROFILING_INFO pLogInfo = ProcmonGetLogEntryAndCopyFrameChain(MONITOR_TYPE_PROFILING, 
			NOTIFY_THREAD_PROFILING,
			pProcessInfo->Seq, 0, 12, &pLogBuf);
		if (pLogInfo) {
			pLogInfo->UserTimeChange = pThreadProfUpdateApc->UserTimeChange;
			pLogInfo->KernelTimeChange = pThreadProfUpdateApc->KernelTimeChange;
			pLogInfo->ContextSwitchesChange = pThreadProfUpdateApc->ContextSwitchesChange;
			ProcmonNotifyProcessLog(pLogBuf);
		}
		DerefProcessInfo(pProcessInfo);
	}

	ExFreeToNPagedLookasideList(&gLookasideListThreadProfiling, pThreadProfUpdateApc);
	*NormalRoutine = NULL;
}

VOID
ProcmonAddToThreadProfileUpdateList(
	_In_ PCLIENT_ID pClientId,
	_In_ ULONG KernelTimeChange,
	_In_ ULONG UserTimeChange,
	_In_ ULONG ContextSwitchesChange
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
	PETHREAD Thread;
	PTHREAD_PROFILING_UPDATE_APC pThreadProfilingUpdateApc;

	Status = PsLookupThreadByThreadId(pClientId->UniqueThread, &Thread);
	if (!NT_SUCCESS(Status)) {
		return;
	}

	pThreadProfilingUpdateApc = ExAllocateFromNPagedLookasideList(&gLookasideListThreadProfiling);
	if (pThreadProfilingUpdateApc) {
		pThreadProfilingUpdateApc->ContextSwitchesChange = ContextSwitchesChange;
		pThreadProfilingUpdateApc->KernelTimeChange = KernelTimeChange;
		pThreadProfilingUpdateApc->ProcessId = pClientId->UniqueProcess;
		pThreadProfilingUpdateApc->UserTimeChange = UserTimeChange;

		KeInitializeApc(&pThreadProfilingUpdateApc->Apc, Thread, OriginalApcEnvironment,
			ProcmonThreadProfileUpdateRoutine, NULL,
			(PKNORMAL_ROUTINE)ProcmonThreadProfileUpdateRoutine, KernelMode, NULL);
		if (!KeInsertQueueApc(&pThreadProfilingUpdateApc->Apc, NULL, NULL, 0)) {
			ExFreeToNPagedLookasideList(&gLookasideListThreadProfiling, pThreadProfilingUpdateApc);
		}
	}

	ObDereferenceObject(Thread);
}

PTHREAD_PROFILING_INFO
ProcmonThreadProfileNotify(
	_In_ PSYSTEM_THREAD_INFORMATION pThreadInfo
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
	PTHREAD_PROFILING_INFO pThreadProfilingInfo;
	ULONG ContextSwitchesChange;
	ULONG UserTimeChange;
	ULONG KernelTimeChange;
	CLIENT_ID pClientId;

	pThreadProfilingInfo = ProcmonGetThreadInfoFromList(&pThreadInfo->ClientId);
	if (pThreadProfilingInfo) {
		if (pThreadProfilingInfo->ClientId.UniqueProcess == pThreadInfo->ClientId.UniqueProcess) {
			if ((pThreadProfilingInfo->KernelTime.QuadPart != pThreadInfo->KernelTime.QuadPart
				|| pThreadProfilingInfo->UserTime.QuadPart != pThreadInfo->UserTime.QuadPart
				|| pThreadProfilingInfo->ContextSwitches != pThreadInfo->ContextSwitches)
				&& (pThreadProfilingInfo->KernelTime.QuadPart ||
					pThreadProfilingInfo->UserTime.QuadPart ||
					pThreadProfilingInfo->ContextSwitches))
			{
				ContextSwitchesChange = pThreadInfo->ContextSwitches - pThreadProfilingInfo->ContextSwitches;
				UserTimeChange = pThreadInfo->UserTime.LowPart - pThreadProfilingInfo->UserTime.LowPart;
				KernelTimeChange = pThreadInfo->KernelTime.LowPart - pThreadProfilingInfo->KernelTime.LowPart;
				pClientId = pThreadInfo->ClientId;
				ProcmonAddToThreadProfileUpdateList(&pClientId, KernelTimeChange, UserTimeChange, ContextSwitchesChange);
			}
		}
		pThreadProfilingInfo->ClientId.UniqueProcess = pThreadInfo->ClientId.UniqueProcess;
		pThreadProfilingInfo->KernelTime.QuadPart = pThreadInfo->KernelTime.QuadPart;
		pThreadProfilingInfo->UserTime.QuadPart = pThreadInfo->UserTime.QuadPart;
		pThreadProfilingInfo->ContextSwitches = pThreadInfo->ContextSwitches;
	}
	return pThreadProfilingInfo;
}

VOID
ProcmonProcessThreadProfilingNotify(
	_In_ BOOLEAN bEnableThreadProfileNotify
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
	BOOLEAN bThreadProfileNotify;
	BOOLEAN bSystemProcessProfileNotify;
	LARGE_INTEGER Timeout;
	NTSTATUS Status;
	PVOID pBuffer = NULL;
	ULONG NeedLength = 0x10000;
	PSYSTEM_PROCESS_INFORMATION pSystemProcessInfo;

	bThreadProfileNotify = bEnableThreadProfileNotify;
	bSystemProcessProfileNotify = !bEnableThreadProfileNotify;
	Timeout.QuadPart = 0;

	if (bEnableThreadProfileNotify) {
		if (KeWaitForSingleObject(&gTimerProcessProfiling, 0, 0, 0, &Timeout) != STATUS_TIMEOUT) {
			bSystemProcessProfileNotify = TRUE;
		}
	}
	else {
		if (KeWaitForSingleObject(&gTimerThreadProfiling, 0, 0, 0, &Timeout) != STATUS_TIMEOUT) {
			bThreadProfileNotify = TRUE;
		}

	}

	//
	// Allocate the buffer
	//

	do
	{
		if (pBuffer) {
			ExFreePoolWithTag(pBuffer, 0);
		}

		pBuffer = ProcmonAllocatePoolWithTag(PagedPool, NeedLength, '5');
		if (!pBuffer) {
			break;
		}

		//
		// try to query 
		//

		Status = ZwQuerySystemInformation(SystemProcessInformation, pBuffer, NeedLength, &NeedLength);
		if (NT_SUCCESS(Status)) {
			break;
		}

	} while (TRUE);

	if (!pBuffer) {
		return;
	}

	pSystemProcessInfo = (PSYSTEM_PROCESS_INFORMATION)pBuffer;
	do
	{

		if (bSystemProcessProfileNotify) {
			ProcmonProcessProfilingNotify(pSystemProcessInfo);
		}

		if (bThreadProfileNotify) {
			if (pSystemProcessInfo->UniqueProcessId &&
				pSystemProcessInfo->UniqueProcessId != gProcessId) {

				PSYSTEM_THREAD_INFORMATION pThreadInfo = (PSYSTEM_THREAD_INFORMATION)(pSystemProcessInfo + 1);
				for (int i = 0; i < (int)pSystemProcessInfo->NumberOfThreads; i++)
				{
					ProcmonThreadProfileNotify(&pThreadInfo[i]);
				}
			}
		}

		if (!pSystemProcessInfo->NextEntryOffset) {
			break;
		}
		pSystemProcessInfo = (PSYSTEM_PROCESS_INFORMATION)((ULONG_PTR)pSystemProcessInfo + pSystemProcessInfo->NextEntryOffset);
	} while (TRUE);

	ExFreePoolWithTag(pBuffer, 0);
}


VOID
ProcmonGetImageRealNameRoutine(
	_Inout_ PVOID Parameter
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
	PGETFULLNAME_WORKITEM pWorkItem = (PGETFULLNAME_WORKITEM)Parameter;
	PUNICODE_STRING pUniStrFullName = pWorkItem->pUniStrFullName;
	PUNICODE_STRING pUniStrImageName = pWorkItem->pUniStrImageName;

	if (ProcmonIsFileInSystemRoot(pUniStrImageName) ||
		ProcmonIsFileExist(pUniStrImageName) ||
		((ProcmonAppendVolumeName(pUniStrImageName, pUniStrFullName), !pUniStrFullName->Buffer) &&
		(ProcmonEnumAllVolumes(), ProcmonAppendVolumeName(pUniStrImageName, pUniStrFullName), !pUniStrFullName->Buffer)))
	{
		pUniStrFullName->Buffer = (PWCH)ProcmonAllocatePoolWithTag(0, pUniStrImageName->Length, 'b');
		if (pUniStrFullName->Buffer) {
			pUniStrFullName->MaximumLength = pUniStrImageName->Length;
			RtlCopyUnicodeString(pUniStrFullName, pUniStrImageName);
		}
	}
	KeSetEvent(&pWorkItem->NotifyEvent, 0, 0);
}

VOID
ProcmonGetImageRealName(
	_In_ PUNICODE_STRING pUniImageName,
	_Out_ PUNICODE_STRING pUniStrFullName
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
	WCHAR *pNewBuf;
	UNICODE_STRING UniImageNameCopy;
	GETFULLNAME_WORKITEM WorkItem;

	pUniStrFullName->Buffer = NULL;
	UniImageNameCopy.Length = pUniImageName->Length;
	pNewBuf = ProcmonAllocatePoolWithTag(0, UniImageNameCopy.Length, 'b');
	UniImageNameCopy.Buffer = pNewBuf;
	if (pNewBuf) {
		RtlCopyMemory(pNewBuf, pUniImageName->Buffer, UniImageNameCopy.Length);
		WorkItem.pUniStrImageName = &UniImageNameCopy;
		WorkItem.pUniStrFullName = pUniStrFullName;
		KeInitializeEvent(&WorkItem.NotifyEvent, 0, 0);
		WorkItem.WorkItem.WorkerRoutine = ProcmonGetImageRealNameRoutine;
		WorkItem.WorkItem.List.Flink = NULL;
		WorkItem.WorkItem.Parameter = &WorkItem;
		if (PsGetCurrentProcessId() == gSystemProcessId)
			ProcmonGetImageRealNameRoutine(&WorkItem);
		else
			ExQueueWorkItem(&WorkItem.WorkItem, DelayedWorkQueue);
		KeWaitForSingleObject(&WorkItem.NotifyEvent, 0, 0, 0, NULL);
		ExFreePoolWithTag(UniImageNameCopy.Buffer, 0);
	}
}

VOID
ProcmonNotifyImageLoad(
	_In_ PUNICODE_STRING pUniStrImageName,
	_In_ LONG Seq,
	_In_ PVOID ImageBase,
	_In_ ULONG ImageSize,
	_In_ USHORT nFrameChainDepth,
	_In_ PVOID pStackFrame
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
	PLOG_BUFFER pLogBuf;

	PLOG_LOADIMAGE_INFO pLogImgInfo = (PLOG_LOADIMAGE_INFO)ProcmonGetLogEntryAndInit(
		MONITOR_TYPE_PROCESS,
		NOTIFY_IMAGE_LOAD,
		Seq,
		0,
		pUniStrImageName->Length + sizeof(LOG_LOADIMAGE_INFO),
		&pLogBuf,
		nFrameChainDepth,
		pStackFrame);
	if (pLogImgInfo) {
		pLogImgInfo->ImageBase = ImageBase;
		pLogImgInfo->ImageSize = ImageSize;
		pLogImgInfo->ImageNameLength = pUniStrImageName->Length >> 1;
		RtlCopyMemory((PVOID)(pLogImgInfo + 1), pUniStrImageName->Buffer, pUniStrImageName->Length);
		ProcmonNotifyProcessLog(pLogBuf);
	}
}

#pragma pack(1)
typedef struct _USER_GETIMAGENAME_MESSAGE
{
	ULONG ProcessId;
	PVOID ImageBase;
}USER_GETIMAGENAME_MESSAGE, *PUSER_GETIMAGENAME_MESSAGE;
#pragma pack()

NTSTATUS
CommunicateWithUserClient(
	_In_ PVOID SenderBuffer,
	_In_ ULONG SenderBufferLength,
	_Out_ PVOID ReplyBuffer,
	_Inout_ PULONG ReplyLength
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
	LARGE_INTEGER Timeout;

	Timeout.QuadPart = -10000000;
	return FltSendMessage(
		gFilterHandle,
		&gClientProcessPathPort,
		SenderBuffer,
		SenderBufferLength,
		ReplyBuffer,
		ReplyLength,
		&Timeout);
}

VOID
ProcmonNotifyImageLoadApcRoutine(
	IN PVOID NormalContext,
	IN PVOID SystemArgument1,
	IN PVOID SystemArgument2
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
	PPROCESSINFO_LIST pProcessInfo;
	PLOADIMAGE_INFO pLoadImageInfo = (PLOADIMAGE_INFO)SystemArgument1;
	HANDLE ProcessId = (HANDLE)SystemArgument2;

	UNREFERENCED_PARAMETER(NormalContext);

	pProcessInfo = RefProcessInfo(ProcessId, 1);
	if (pProcessInfo)
	{
		PFLT_FILE_NAME_INFORMATION pFileNameInfo = pLoadImageInfo->pFileNameInfo;
		if (pFileNameInfo) {
			ProcmonNotifyImageLoad(
				&pFileNameInfo->Name,
				pProcessInfo->Seq,
				pLoadImageInfo->ImageInfo.ImageBase,
				(ULONG)pLoadImageInfo->ImageInfo.ImageSize,
				pLoadImageInfo->StackFrameCounts,
				pLoadImageInfo->StackFrameChain);
			FltReleaseFileNameInformation(pLoadImageInfo->pFileNameInfo);
		}
		else {
			PUNICODE_STRING pImageFileName = (PUNICODE_STRING)ProcmonAllocatePoolWithTag(NonPagedPool, 0x8010, '9');
			if (pImageFileName) {
				USER_GETIMAGENAME_MESSAGE Message;
				ULONG ReplayBufferLen = 0x8010;
				Message.ProcessId = (ULONG)(ULONG_PTR)ProcessId;
				Message.ImageBase = pLoadImageInfo->ImageInfo.ImageBase;
				pImageFileName->Length = 0;
				Status = CommunicateWithUserClient(&Message, sizeof(USER_GETIMAGENAME_MESSAGE),
					pImageFileName, &ReplayBufferLen);
				if (NT_SUCCESS(Status) && pImageFileName->Length)
				{
					pImageFileName->Buffer = (PWCH)(pImageFileName + 1);
					ProcmonNotifyImageLoad(
						pImageFileName,
						pProcessInfo->Seq,
						pLoadImageInfo->ImageInfo.ImageBase,
						(ULONG)pLoadImageInfo->ImageInfo.ImageSize,
						pLoadImageInfo->StackFrameCounts,
						pLoadImageInfo->StackFrameChain);
				}
				ExFreePoolWithTag(pImageFileName, 0);
			}
		}
		DerefProcessInfo(pProcessInfo);
	}
	ExFreePoolWithTag(pLoadImageInfo, 0);
}

VOID
ProcmonQueueApcSpecialApc(
	IN PKAPC Apc,
	IN PKNORMAL_ROUTINE *NormalRoutine,
	IN PVOID *NormalContext,
	IN PVOID *SystemArgument1,
	IN PVOID *SystemArgument2
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
	PAGED_CODE();

	UNREFERENCED_PARAMETER(NormalRoutine);
	UNREFERENCED_PARAMETER(NormalContext);
	UNREFERENCED_PARAMETER(SystemArgument1);
	UNREFERENCED_PARAMETER(SystemArgument2);

	ExFreePool(Apc);
}


VOID
LoadImageNotifyRoutine(
	_In_ PUNICODE_STRING FullImageName,
	_In_ HANDLE ProcessId,
	_In_ PIMAGE_INFO pImageInfo
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
	UNICODE_STRING UniStrImageName = {0};
	PEPROCESS Process;
	PPROCESSINFO_LIST pProcessInfo;

	if (!FullImageName) {
		return;
	}

	Process = IoGetCurrentProcess();
	if (Process == gCurrentProcess || !(gFlags & 1)) {
		return;
	}

	if (!FullImageName->Length) {
		FullImageName->Length = FullImageName->MaximumLength;
	}

	if (pImageInfo->SystemModeImage) {
		if (gCurrentProcess) {
			UniStrImageName.MaximumLength = FullImageName->Length;
			UniStrImageName.Buffer = ProcmonAllocatePoolWithTag(0, FullImageName->Length, 'C');;
			if (!UniStrImageName.Buffer)
				return;
			RtlCopyUnicodeString(&UniStrImageName, FullImageName);
		}
	}else{
		if (gCurrentProcess) {
			PKAPC Apc = (PKAPC)ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(KAPC), 'A');
			if (Apc) {
				PLOADIMAGE_INFO pLoadImageInfo = (PLOADIMAGE_INFO)ProcmonAllocatePoolWithTag(0, sizeof(LOADIMAGE_INFO), 'B');
				if (pLoadImageInfo) {
					pLoadImageInfo->pFileNameInfo = NULL;

					if (pImageInfo->ExtendedInfoPresent) {
						GETFILENAME_WORKITEM WorkItem;
						PIMAGE_INFO_EX pImageInfoEx = CONTAINING_RECORD(pImageInfo, IMAGE_INFO_EX, ImageInfo);

						KeInitializeEvent(&WorkItem.NotifyEvent, 0, 0);
						WorkItem.FileObject = pImageInfoEx->FileObject;
						WorkItem.WorkItem.WorkerRoutine = ProcmonGetFileNameInfoWorkRoutine;
						WorkItem.WorkItem.List.Flink = NULL;
						WorkItem.WorkItem.Parameter = &WorkItem;
						ExQueueWorkItem(&WorkItem.WorkItem, DelayedWorkQueue);
						KeWaitForSingleObject(&WorkItem.NotifyEvent, 0, 0, 0, NULL);
						if (NT_SUCCESS(WorkItem.Status)) {
							pLoadImageInfo->pFileNameInfo = WorkItem.pFileNameInfo;
						}
					}

					pLoadImageInfo->ImageInfo = *pImageInfo;
					pLoadImageInfo->StackFrameCounts = (USHORT)ProcmonGenStackFrameChain(TRUE, 
						pLoadImageInfo->StackFrameChain, MAX_STACKFRAME_COUNTS);
					KeInitializeApc(Apc, KeGetCurrentThread(), OriginalApcEnvironment, ProcmonQueueApcSpecialApc, NULL,
						ProcmonNotifyImageLoadApcRoutine, 0, NULL);
					KeInsertQueueApc(Apc, pLoadImageInfo, ProcessId, 0);
				}
			}
			return;
		}

		ProcmonGetImageRealName(FullImageName, &UniStrImageName);
	}

	if (UniStrImageName.Buffer) {
		if (pImageInfo->Properties & 0x100)
			ProcessId = gSystemProcessId;
		else
			ProcessId = PsGetCurrentProcessId();

		pProcessInfo = RefProcessInfo(ProcessId, TRUE);
		if (pProcessInfo) {

			PLOG_BUFFER pLogBuf;
			PLOG_LOADIMAGE_INFO pLogImgInfo = (PLOG_LOADIMAGE_INFO)ProcmonGetLogEntryAndCopyFrameChain(
				MONITOR_TYPE_PROCESS,
				NOTIFY_IMAGE_LOAD,
				pProcessInfo->Seq,
				0,
				UniStrImageName.Length + sizeof(LOG_LOADIMAGE_INFO),
				&pLogBuf);
			if (pLogImgInfo) {
				pLogImgInfo->ImageBase = pImageInfo->ImageBase;
				pLogImgInfo->ImageSize = (ULONG)pImageInfo->ImageSize;
				pLogImgInfo->ImageNameLength = UniStrImageName.Length >> 1;
				RtlCopyMemory(pLogImgInfo + 1, UniStrImageName.Buffer, UniStrImageName.Length);
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}

		ExFreePoolWithTag(UniStrImageName.Buffer, 0);
	}

}


VOID
ThreadSystemModuleMonitor(
	_In_ PVOID StartContext
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
	PVOID Objects[4];
	KWAIT_BLOCK WaitBlockArray[4];
	LARGE_INTEGER DueTime = { 0 };

	UNREFERENCED_PARAMETER(StartContext);

	KeSetTimerEx(&gTimerProcessProfiling, DueTime, 1000, NULL);

	Objects[ProfilingExitEvent] = &gModuleMonitorExitEvent;
	Objects[ProfilingProcess] = &gTimerProcessProfiling;
	Objects[ProfilingThread] = &gTimerThreadProfiling;
	Objects[ProfilingReset] = &gEventProfilingReset;

	while (TRUE)
	{
		Status = KeWaitForMultipleObjects(4, Objects, WaitAny, 0, KernelMode, FALSE, NULL, WaitBlockArray);
		if (Status == ProfilingExitEvent) {
			break;
		}
		else if (Status == ProfilingProcess) {
			ProcmonProcessThreadProfilingNotify(1);
		}
		else if (Status == ProfilingThread) {
			ProcmonProcessThreadProfilingNotify(0);
		}
		else if (Status == ProfilingReset) {
			for (int i = 0; i < 0x100; i++)
			{
				PLIST_ENTRY pListHead = &gListEntryArray[i];
				while (pListHead->Flink != pListHead)
				{
					PLIST_ENTRY pEntry = RemoveHeadList(pListHead);
					PTHREAD_PROFILING_INFO pThreadProfilingInfo = CONTAINING_RECORD(pEntry, THREAD_PROFILING_INFO, List);
					ExFreePoolWithTag(pThreadProfilingInfo, 0);
				}
			}
		}
	}


	//
	// Reset All
	//

	for (int i = 0; i < 0x100; i++)
	{
		PLIST_ENTRY pListHead = &gListEntryArray[i];
		while (pListHead->Flink != pListHead)
		{
			PLIST_ENTRY pEntry = RemoveHeadList(pListHead);
			PTHREAD_PROFILING_INFO pThreadProfilingInfo = CONTAINING_RECORD(pEntry, THREAD_PROFILING_INFO, List);
			ExFreePoolWithTag(pThreadProfilingInfo, 0);
		}
	}

}

NTSTATUS
EnableProcessMonitor(
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
	NTSTATUS Status = STATUS_SUCCESS;

	if (bEnable != gThreadMonitorEnable) {
		if (bEnable) {
			if (!fnPsSetCreateThreadNotifyRoutineEx ||
				(Status = fnPsSetCreateThreadNotifyRoutineEx(PsCreateThreadNotifySubsystems, 
				(PVOID)CreateThreadNotifyRoutine), 
					!NT_SUCCESS(Status))) {
				Status = PsSetCreateThreadNotifyRoutine(CreateThreadNotifyRoutine);
				gThreadMonitorEnable = bEnable;
			}
		}else{
			Status = PsRemoveCreateThreadNotifyRoutine(CreateThreadNotifyRoutine);
			gThreadMonitorEnable = bEnable;
		}
	}

	if (bEnable != gProcessMonitorEnable) {
		if (fnPsSetCreateProcessNotifyRoutineEx2) {
			Status = fnPsSetCreateProcessNotifyRoutineEx2(PsCreateProcessNotifySubsystems, 
				(PVOID)CreateProcessNotifyRoutineEx2, bEnable == 0);
		}else{
			Status = PsSetCreateProcessNotifyRoutine(CreateProcessNotifyRoutine, bEnable == 0);
		}
		gProcessMonitorEnable = bEnable;
	}

#if 0
	if (bEnable != gSystemModuleLoadMonitorEnable) {
		if (bEnable){
			OBJECT_ATTRIBUTES ObjectAttributes;
			InitializeObjectAttributes(&ObjectAttributes, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);
			PsCreateSystemThread(&ghThreadModuleMonitor, 0x1F03FFu, &ObjectAttributes, NULL, NULL, 
				ThreadSystemModuleMonitor, NULL);
		}else{
			KeSetEvent(&gModuleMonitorExitEvent, 0, 0);
			ZwWaitForSingleObject(ghThreadModuleMonitor, FALSE, NULL);
			ZwClose(ghThreadModuleMonitor);
		}
		gSystemModuleLoadMonitorEnable = bEnable;
	}
#endif

	if (bEnable) {
		if (!gCurrentProcess) {
			PPROCESSINFO_LIST pProcessInfo = RefProcessInfo(gSystemProcessId, FALSE);
			if (pProcessInfo) {
				PPROCESS_FULL_INFO pProcessFullInfo = ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(PROCESS_FULL_INFO), '8');
				pProcessInfo->pProcessFullInfo = pProcessFullInfo;
				if (pProcessFullInfo){
					pProcessFullInfo->pParentProcessInfo = NULL;
					pProcessInfo->pProcessFullInfo->StackFrameCounts = 0;
					pProcessInfo->pProcessFullInfo->ImageFileName.MaximumLength = 0;
					pProcessInfo->pProcessFullInfo->ImageFileName.Length = 0;
					pProcessInfo->pProcessFullInfo->CommandLine.MaximumLength = 0;
					pProcessInfo->pProcessFullInfo->CommandLine.Length = 0;
				}
				DerefProcessInfo(pProcessInfo);
			}
		}
		PPROCESSINFO_LIST pProcessInfo2 = RefProcessInfo(gSystemProcessId, TRUE);
		DerefProcessInfo(pProcessInfo2);
	}

	if (bEnable != gLoadImageMointorEnable){
		if (bEnable) {
			Status = PsSetLoadImageNotifyRoutine(LoadImageNotifyRoutine);
		}else{
			Status = PsRemoveLoadImageNotifyRoutine(LoadImageNotifyRoutine);
		}
		gLoadImageMointorEnable = bEnable;
	}

	if (!bEnable && !gCurrentProcess)
		FreeAllProcessInfo();
	return Status;
}


VOID
ProcmonCollectProcessAndSystemPerformanceData(
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
	NTSTATUS ExitStatus = STATUS_SUCCESS;
	PLIST_ENTRY pEntry;
	HANDLE ProcessId;
	PPROCESSINFO_LIST pProcessInfo;
	PLOG_BUFFER pLogBuf;

	ExAcquireFastMutex(&gProcessListMutex);

	for (pEntry = gProcessInfoList.Flink;
		pEntry != &gProcessInfoList;
		pEntry = pEntry->Flink)
	{
		pProcessInfo = CONTAINING_RECORD(pEntry, PROCESSINFO_LIST, List);
		if (pProcessInfo->ProcessId) {
			KERNEL_USER_TIMES KernelUserTime = { 0 };
			VM_COUNTERS VmCounters = { 0 };
			PROCESS_BASIC_INFORMATION ProcessBasicInfo;
			CLIENT_ID ClientId;
			OBJECT_ATTRIBUTES ObjectAttributes;
			HANDLE hProcess;

			ClientId.UniqueThread = 0;
			ClientId.UniqueProcess = pProcessInfo->ProcessId;
			InitializeObjectAttributes(&ObjectAttributes, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);
			Status = ZwOpenProcess(&hProcess, 0, &ObjectAttributes, &ClientId);
			if (NT_SUCCESS(Status) && hProcess) {
				if (NT_SUCCESS(ZwQueryInformationProcess(hProcess, ProcessBasicInformation,
					&ProcessBasicInfo, sizeof(ProcessBasicInfo), NULL))) {
					ExitStatus = ProcessBasicInfo.ExitStatus;
					ZwQueryInformationProcess(hProcess, ProcessTimes, &KernelUserTime, sizeof(KERNEL_USER_TIMES), NULL);
					ZwQueryInformationProcess(hProcess, ProcessVmCounters, &VmCounters, sizeof(VM_COUNTERS), NULL);
					ZwClose(hProcess);
				}
			}

			if (NT_SUCCESS(ExitStatus)) {
				PLOG_PROCESSBASIC_INFO pLogBaiscInfo = (PLOG_PROCESSBASIC_INFO)ProcmonGetLogEntryAndInit(
					MONITOR_TYPE_PROCESS, NOTIFY_PROCESS_PERFORMANCE, pProcessInfo->Seq, 0, sizeof(LOG_PROCESSBASIC_INFO), &pLogBuf, 0, NULL);
				if (pLogBaiscInfo) {
					pLogBaiscInfo->ExitStatus = 0;
					pLogBaiscInfo->KenrnelTime.QuadPart = KernelUserTime.KernelTime.QuadPart;
					pLogBaiscInfo->UserTime.QuadPart = KernelUserTime.UserTime.QuadPart;
					pLogBaiscInfo->PagefileUsage = VmCounters.PagefileUsage;
					pLogBaiscInfo->PeakPagefileUsage = VmCounters.PeakPagefileUsage;
					pLogBaiscInfo->WorkingSetSize = VmCounters.WorkingSetSize;
					pLogBaiscInfo->PeakWorkingSetSize = VmCounters.PeakWorkingSetSize;
					ProcmonNotifyProcessLog(pLogBuf);
				}
			}
		}
	}
	ExReleaseFastMutex(&gProcessListMutex);
	ProcessId = PsGetCurrentProcessId();
	pProcessInfo = RefProcessInfo(ProcessId, TRUE);
	if (pProcessInfo) {
		SYSTEM_PERFORMANCE_INFORMATION SystemPerfInfo;
		ULONG ReturnLength = sizeof(SystemPerfInfo);

		Status = ZwQuerySystemInformation(SystemPerformanceInformation, &SystemPerfInfo, sizeof(SystemPerfInfo), &ReturnLength);
		if (NT_SUCCESS(Status)) {
			PLOG_SYSTEMPERF_INFO pLogSysPrefInfo = (PLOG_SYSTEMPERF_INFO)ProcmonGetLogEntryAndCopyFrameChain(
				MONITOR_TYPE_PROCESS, NOTIFY_SYSTEM_PERFORMANCE, 
				pProcessInfo->Seq, 0, sizeof(LOG_SYSTEMPERF_INFO), &pLogBuf);
			if (pLogSysPrefInfo) {
				pLogSysPrefInfo->UnKnown = 0x1000;
				pLogSysPrefInfo->SystemPerfInfo = SystemPerfInfo;
				ProcmonNotifyProcessLog(pLogBuf);
			}
		}

		DerefProcessInfo(pProcessInfo);
	}
}

BOOLEAN
ProcmonEnableThreadProfiling(
	_In_ LARGE_INTEGER Period
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
	LARGE_INTEGER DueTime;

	DueTime.QuadPart = 0;

	if (DueTime.HighPart)
		return KeSetTimerEx(
			&gTimerThreadProfiling,
			DueTime,
			(LONG)(Period.QuadPart / 10000),
			NULL);
	KeSetEvent(&gEventProfilingReset, 0, 0);
	return KeCancelTimer(&gTimerThreadProfiling);
}