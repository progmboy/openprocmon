#pragma once

#include <ntifs.h>
#include <fltKernel.h>

struct _PROCESSINFO_LIST;

typedef struct _PROCESS_FULL_INFO
{
	struct _PROCESSINFO_LIST* pParentProcessInfo;
	USHORT StackFrameCounts;
	PVOID StackFrame[MAX_STACKFRAME_COUNTS];
	UNICODE_STRING ImageFileName;
	UNICODE_STRING CommandLine;
}PROCESS_FULL_INFO, *PPROCESS_FULL_INFO;

typedef struct _PROCESSINFO_LIST
{
	/*00*/PVOID Process;
	/*08*/LONG RefCount;
	/*0C*/LONG Seq;
	/*10*/HANDLE ProcessId;
	/*18*/BOOLEAN bInit;
	/*20*/PPROCESS_FULL_INFO pProcessFullInfo;
	/*28*/LIST_ENTRY List;
	/*38*/LIST_ENTRY ProcessExitList;
}PROCESSINFO_LIST, *PPROCESSINFO_LIST;

typedef struct _LOADIMAGE_INFO
{
	/*00*/IMAGE_INFO ImageInfo;
	/*28*/PFLT_FILE_NAME_INFORMATION pFileNameInfo;
	/*30*/USHORT StackFrameCounts;
	/*32*/UCHAR Fill32[0x6];
	/*38*/PVOID StackFrameChain[MAX_STACKFRAME_COUNTS];
}LOADIMAGE_INFO, *PLOADIMAGE_INFO;

typedef struct _THREADINFO_LIST
{
	ULONG RefCount;
	PETHREAD Thread;
	LIST_ENTRY List;
}THREADINFO_LIST, *PTHREADINFO_LIST;

typedef struct _THREAD_PROFILING_UPDATE_APC
{
	KAPC Apc;
	HANDLE ProcessId;
	ULONG ContextSwitchesChange;
	ULONG KernelTimeChange;
	ULONG UserTimeChange;
}THREAD_PROFILING_UPDATE_APC, *PTHREAD_PROFILING_UPDATE_APC;

typedef enum _THREAD_PROFILING_WAIT_OBJECTS
{
	ProfilingExitEvent = 0,
	ProfilingProcess,
	ProfilingThread,
	ProfilingReset
}THREAD_PROFILING_WAIT_OBJECTS;

typedef struct _THREAD_PROFILING_INFO
{
	CLIENT_ID ClientId;
	ULONG ContextSwitches;
	ULONG Fill14;
	LARGE_INTEGER KernelTime;
	LARGE_INTEGER UserTime;
	LIST_ENTRY List;
}THREAD_PROFILING_INFO, *PTHREAD_PROFILING_INFO;

typedef struct _GETFULLNAME_WORKITEM
{
	WORK_QUEUE_ITEM WorkItem;
	KEVENT NotifyEvent;
	PUNICODE_STRING pUniStrImageName;
	PUNICODE_STRING pUniStrFullName;
}GETFULLNAME_WORKITEM, *PGETFULLNAME_WORKITEM;

typedef
NTSTATUS
(NTAPI *FNZwQueryInformationThread)(
	__in HANDLE ThreadHandle,
	__in THREADINFOCLASS ThreadInformationClass,
	__out_bcount(ThreadInformationLength) PVOID ThreadInformation,
	__in ULONG ThreadInformationLength,
	__out_opt PULONG ReturnLength
	);

typedef
NTSTATUS
(NTAPI *FNSeLocateProcessImageName)(
	_Inout_ PEPROCESS Process,
	_Outptr_ PUNICODE_STRING *pImageFileName
	);

typedef
NTSTATUS
(NTAPI *FNPsSetCreateThreadNotifyRoutineEx)(
	_In_ PSCREATETHREADNOTIFYTYPE NotifyType,
	_In_ PVOID NotifyInformation
	);


typedef
NTSTATUS
(NTAPI *FNPsSetCreateProcessNotifyRoutineEx2)(
	_In_ PSCREATEPROCESSNOTIFYTYPE NotifyType,
	_In_ PVOID NotifyInformation,
	_In_ BOOLEAN Remove
	);

typedef
NTSTATUS
(NTAPI *FNZwOpenProcessTokenEx)(
	_In_ HANDLE ProcessHandle,
	_In_ ACCESS_MASK DesiredAccess,
	_In_ ULONG HandleAttributes,
	_Out_ PHANDLE TokenHandle
	);


PPROCESSINFO_LIST
RefProcessInfo(
	_In_ HANDLE ProcessId,
	_In_ BOOLEAN bNotAsyn
);

VOID
DerefProcessInfo(
	_In_ PPROCESSINFO_LIST pProcessInfo
);

BOOLEAN
RefThreadInfo(
	VOID
);

VOID
DeRefThreadInfo(
	VOID
);

BOOLEAN
ProcmonEnableThreadProfiling(
	_In_ LARGE_INTEGER Period
);

VOID
ProcmonCollectProcessAndSystemPerformanceData(
	VOID
);

VOID
ProcmonWaitProcessExitWorkRoutine(
	_In_ PVOID pWorkItem
);

VOID
ProcmonProcessMonitorInit(
	VOID
);

NTSTATUS
EnableProcessMonitor(
	_In_ BOOLEAN bEnable
);

VOID
LoadImageNotifyRoutine(
	_In_ PUNICODE_STRING FullImageName,
	_In_ HANDLE ProcessId,
	_In_ PIMAGE_INFO pImageInfo
);