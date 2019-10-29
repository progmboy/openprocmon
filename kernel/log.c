
#include "log.h"
#include "globals.h"
#include "utils.h"
#include "process.h"

// #ifdef ALLOC_PRAGMA
// #pragma alloc_text(PAGE, ProcmonGenStackFrameChain)
// #pragma alloc_text(PAGE, ProcmonGetLogEntryAndCopyFrameChain)
// #pragma alloc_text(PAGE, ProcmonGetLogBufferAndLock)
// #pragma alloc_text(PAGE, ProcmonGetLogEntryAndInit)
// #endif

FAST_MUTEX gMutexLogList;
LIST_ENTRY gLogListHead;
LONG gRecordSequence = 0;
NPAGED_LOOKASIDE_LIST gNPagedLooksideListLogBuffer;
LARGE_INTEGER gMonitorStartCounter;
LARGE_INTEGER gPerformanceFrequency;
LARGE_INTEGER gMonitorStartTime;
LONGLONG gWriteFileFrqs;


ULONG
ProcmonGenStackFrameChain(
	_In_ BOOLEAN bRefThread,
	_Out_ PVOID *Callers,
	_In_ USHORT nCounts
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
	int Index;
	USHORT nCaptured;

	nCaptured = (USHORT)RtlWalkFrameChain(Callers, nCounts, 0);

	//
	// Remove All record which called by ourself
	//

	if (gSelfImageBase) {
		for (Index = 0; Index < nCaptured; Index++)
		{
			ULONG_PTR pCallerRip = (ULONG_PTR)Callers[Index];
			if (pCallerRip <= (ULONG_PTR)gSelfImageBase ||
				pCallerRip >= (ULONG_PTR)gSelfImageBase + gSelfImageSize) {
				break;
			}
		}

		if (Index) {
			memmove(Callers, &Callers[Index], (nCounts - Index) * sizeof(PVOID));
			nCaptured -= (USHORT)Index;
		}
	}

	if (!bRefThread) {
		return nCaptured;
	}

	if (!RefThreadInfo()) {
		ULONG_PTR LowLimit, HighLimit;
		IoGetStackLimits(&LowLimit, &HighLimit);
		if ((ULONG_PTR)&HighLimit - LowLimit > PAGE_SIZE) {
			nCaptured += (USHORT)RtlWalkFrameChain(&Callers[nCaptured], nCounts - nCaptured, 1);
			DeRefThreadInfo();
			return nCaptured;
		}
	}

	DeRefThreadInfo();
	return 0;
}

PVOID
ProcmonGetLogEntryAndCopyFrameChain(
	_In_ UCHAR MonitorType,
	_In_ USHORT NotifyType,
	_In_ LONG Sequence,
	_In_ NTSTATUS Status,
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER *ppLogBuffer
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
	USHORT nFrameChainCounts;
	PVOID StackFrameChain[MAX_STACKFRAME_COUNTS];

	nFrameChainCounts = (USHORT)ProcmonGenStackFrameChain(TRUE, StackFrameChain, MAX_STACKFRAME_COUNTS);
	return ProcmonGetLogEntryAndInit(
		MonitorType,
		NotifyType,
		Sequence,
		Status,
		Length,
		ppLogBuffer,
		nFrameChainCounts,
		StackFrameChain);
}

PVOID
ProcmonGetLogEntryAndSeq(
	_In_ BOOLEAN bRefThread,
	_In_ UCHAR MonitorType,
	_In_ USHORT NotifyType,
	_In_ LONG Sequence,
	_In_ NTSTATUS Status,
	_In_ ULONG Length,
	_Out_ PLONG pRecordSequence,
	_Out_ PLOG_BUFFER *ppLogBuffer
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
	PVOID pBuffer;
	USHORT nFrameChainCounts;
	PVOID StackFrameChain[MAX_STACKFRAME_COUNTS];

	//*pRecordSequence = -1;

	nFrameChainCounts = (USHORT)ProcmonGenStackFrameChain(bRefThread, StackFrameChain, MAX_STACKFRAME_COUNTS);
	pBuffer = ProcmonGetLogEntryAndInit(
		MonitorType,
		NotifyType,
		Sequence,
		Status,
		Length,
		ppLogBuffer,
		nFrameChainCounts,
		StackFrameChain);

	if (pBuffer) {
		*pRecordSequence = ((PLOG_ENTRY)((ULONG_PTR)pBuffer - sizeof(PVOID) * nFrameChainCounts - sizeof(LOG_ENTRY)))->Sequence;
	}
	return pBuffer;
}

PVOID
ProcmonGetLogBufferAndLock(
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER* ppLogBuffer
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
	PLOG_BUFFER pLogBuffer;
	LARGE_INTEGER Time;

	//
	// Lock
	//

	ExAcquireFastMutex(&gMutexLogList);

	if (!IsListEmpty(&gLogListHead)) {
		pLogBuffer = CONTAINING_RECORD(gLogListHead.Blink, LOG_BUFFER, List);
		if (MAX_PROCMON_MESSAGE_LEN - pLogBuffer->Length >= Length) {
			pLogBuffer->Length += Length;
			*ppLogBuffer = pLogBuffer;
			return &pLogBuffer->Text[pLogBuffer->Length - Length];
		}else{
			pLogBuffer = CONTAINING_RECORD(gLogListHead.Flink, LOG_BUFFER, List);
			KeQuerySystemTime(&Time);
			
			//
			// 如果写文件速度太快则不分配内存
			//
			
			if (Time.QuadPart - pLogBuffer->DataTime.QuadPart > gWriteFileFrqs){
				ExReleaseFastMutex(&gMutexLogList);
				*ppLogBuffer = NULL;
				return NULL;
			}
		}
	}

	pLogBuffer = ExAllocateFromNPagedLookasideList(&gNPagedLooksideListLogBuffer);
	if (pLogBuffer) {
		KeQuerySystemTime(&pLogBuffer->DataTime);
		pLogBuffer->Length = 0;
		InsertTailList(&gLogListHead, &pLogBuffer->List);
		pLogBuffer->Length += Length;
		*ppLogBuffer = pLogBuffer;
		return &pLogBuffer->Text[pLogBuffer->Length - Length];
	}

	ExReleaseFastMutex(&gMutexLogList);
	*ppLogBuffer = NULL;
	return 0;
}

// #define ALIGN_DOWN(length, type) \
//     ((ULONG)(length) & ~(sizeof(type) - 1))
// #define ALIGN_UP(length, type) \
//     (ALIGN_DOWN(((ULONG)(length) + sizeof(type) - 1), type))
// #define TIME_OFF(_Time, _PerformanceCounter, _PerformanceFrequency)	\
// 	KeQueryPerformanceCounter(NULL).QuadPart - _PerformanceCounter.QuadPart

LARGE_INTEGER
ProcmonGetTime(
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
	LARGE_INTEGER Time;
	LARGE_INTEGER CounterNow = KeQueryPerformanceCounter(NULL);
	LONGLONG CounterOff = CounterNow.QuadPart - gMonitorStartCounter.QuadPart;

	Time.QuadPart = gMonitorStartTime.QuadPart +
	(10000000 * (CounterOff / gPerformanceFrequency.QuadPart)) +
		((10000000 * (CounterOff % gPerformanceFrequency.QuadPart)) / gPerformanceFrequency.QuadPart);

	return Time;
}

PVOID
ProcmonGetLogEntryAndInit(
	_In_ UCHAR MonitorType,
	_In_ USHORT NotifyType,
	_In_ LONG Sequence,
	_In_ NTSTATUS Status,
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER *ppLogBuffer,
	_In_ USHORT FrameChainDepth,
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
	LONG DstSequence;
	PLOG_ENTRY pLogEntry;

	Length = ALIGN_UP(Length, 4);
	pLogEntry = (PLOG_ENTRY)ProcmonGetLogBufferAndLock(Length + sizeof(PVOID) * FrameChainDepth + sizeof(LOG_ENTRY), 
		ppLogBuffer);
	if (pLogEntry) {
		if (MonitorType == MONITOR_TYPE_POST) {
			DstSequence = Sequence;
		}else{
			if (MonitorType == MONITOR_TYPE_PROCESS && NotifyType == NOTIFY_PROCESS_INIT) {
				DstSequence = gRecordSequence;
			}else{
				DstSequence = InterlockedIncrement(&gRecordSequence);
			}
		}

		pLogEntry->Sequence = DstSequence;
		pLogEntry->NotifyType = NotifyType;
		pLogEntry->field_A = 0;
		pLogEntry->MonitorType = MonitorType;
		pLogEntry->ProcessSeq = Sequence;
		pLogEntry->Status = Status;
		pLogEntry->DataLength = Length;
		pLogEntry->ThreadId = (ULONG)(ULONG_PTR)PsGetCurrentThreadId();
		pLogEntry->nFrameChainCounts = FrameChainDepth;
		pLogEntry->Time = ProcmonGetTime();
		RtlCopyMemory(pLogEntry + 1, pStackFrame, sizeof(PVOID) * FrameChainDepth);
		return (PVOID)((ULONG_PTR)(pLogEntry + 1) + sizeof(PVOID) * FrameChainDepth);

	}

	return NULL;
}

PVOID
ProcmonGetPostLogEntry(
	_In_ LONG Sequence, 
	_In_ NTSTATUS Status, 
	_In_ ULONG Length, 
	_Out_ PLOG_BUFFER *ppLogBuffer
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
	return ProcmonGetLogEntryAndInit(MONITOR_TYPE_POST, 0, Sequence, Status, Length, ppLogBuffer, 0, NULL);
}

VOID
ProcmonNotifyProcessLog(
	_In_ PLOG_BUFFER pLogBuf
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
	if (pLogBuf) {
		if (gbReady) {
			if (!gbFinish && gLogListHead.Flink == &pLogBuf->List && pLogBuf->Length > 0x10000) {
				KeCancelTimer(&gTimerProcessLogData);

				//
				// 立即处理
				//

				KeSetEvent(&gEventProcessData, 0, 0);
				gbFinish = TRUE;
			}
		}
		else {
			LARGE_INTEGER DueTime;
			DueTime.QuadPart = -2500000;

			//
			// 使用Timer进行处理
			//

			KeSetTimer(&gTimerProcessLogData, DueTime, &gDpcProcessData);
		}
		gbReady = TRUE;
		ExReleaseFastMutex(&gMutexLogList);
	}
}

NTSTATUS
ProcessLogDataWithCallback(
	_In_ FNWRITEMSGCALLBACK Callback
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
	PLIST_ENTRY pEntry = NULL;

	ExAcquireFastMutex(&gMutexLogList);

	if (IsListEmpty(&gLogListHead) ||
		(pEntry = RemoveHeadList(&gLogListHead), IsListEmpty(&gLogListHead))) {
		Status = STATUS_END_OF_FILE;
		gbReady = FALSE;
		gbFinish = FALSE;
		KeClearEvent(&gEventProcessData);
	}
	ExReleaseFastMutex(&gMutexLogList);

	if (pEntry) {
		PLOG_BUFFER pBuffer = CONTAINING_RECORD(pEntry, LOG_BUFFER, List);
		Callback(&pBuffer->Length, pBuffer->Length + sizeof(ULONG));
		ExFreeToNPagedLookasideList(&gNPagedLooksideListLogBuffer, pBuffer);
	}
	return Status;
}

VOID 
ProcmonCleanupWriteState(
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
	ExAcquireFastMutex(&gMutexLogList);
	gbReady = FALSE;
	gbFinish = FALSE;
	KeClearEvent(&gEventProcessData);
	ExReleaseFastMutex(&gMutexLogList);
}