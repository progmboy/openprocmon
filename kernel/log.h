#pragma once

#include <ntifs.h>
#include <fltKernel.h>
#include "ntheader.h"
#include "logsdk.h"

#define MAX_STACKFRAME_COUNTS 150

extern FAST_MUTEX gMutexLogList;
extern LIST_ENTRY gLogListHead;
extern LONG gRecordSequence;
extern NPAGED_LOOKASIDE_LIST gNPagedLooksideListLogBuffer;
extern LARGE_INTEGER gMonitorStartCounter;
extern LARGE_INTEGER gPerformanceFrequency;
extern LARGE_INTEGER gMonitorStartTime;
extern LONGLONG gWriteFileFrqs;

typedef
NTSTATUS
(NTAPI *FNWRITEMSGCALLBACK)(
	_In_ PVOID SenderBuffer,
	_In_ ULONG SenderBufferLength
	);

ULONG
ProcmonGenStackFrameChain(
	_In_ BOOLEAN bRefThread,
	_Out_ PVOID *Callers,
	_In_ USHORT nCounts
);

PVOID
ProcmonGetLogEntryAndCopyFrameChain(
	_In_ UCHAR MonitorType,
	_In_ USHORT NotifyType,
	_In_ LONG Sequence,
	_In_ NTSTATUS Status,
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER *ppLogBuffer
);

PVOID
ProcmonGetLogBufferAndLock(
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER* ppLogBuffer
);

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
);

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
);

LARGE_INTEGER
ProcmonGetTime(
	VOID
);

PVOID
ProcmonGetPostLogEntry(
	_In_ LONG Sequence,
	_In_ NTSTATUS Status,
	_In_ ULONG Length,
	_Out_ PLOG_BUFFER *ppLogBuffer
);

VOID
ProcmonNotifyProcessLog(
	_In_ PLOG_BUFFER pLogBuf
);

NTSTATUS
ProcessLogDataWithCallback(
	_In_ FNWRITEMSGCALLBACK Callback
);

VOID
ProcmonCleanupWriteState(
	VOID
);