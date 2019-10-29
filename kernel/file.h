#pragma once

#include <ntifs.h>
#include "process.h"


typedef struct _PAGING_FILEINFO_LIST
{
	struct _PAGING_FILEINFO_LIST* Next;
	PFILE_OBJECT FileObject;
	UNICODE_STRING FileName;
}PAGING_FILEINFO_LIST, *PPAGING_FILEINFO_LIST;

typedef struct _VOLUME_INFO
{
	UNICODE_STRING Name;
	ULONG Type;
	struct _VOLUME_INFO* Next;
}VOLUME_INFO, *PVOLUME_INFO;

typedef struct _FILEOPT_WORKQUEUEITEM
{
	WORK_QUEUE_ITEM WorkItem;
	PETHREAD Thread;
	UCHAR MajorFunction;
	LARGE_INTEGER Time;
	IO_STATUS_BLOCK IoStatus;
	PVOID CompletionContext;
	ULONG Flags;
}FILEOPT_WORKQUEUEITEM, *PFILEOPT_WORKQUEUEITEM;

NTSTATUS
EnableFileMonitor(
	_In_ BOOLEAN bEnable
);

VOID
ProcmonEnumAllVolumes(
	VOID
);

BOOLEAN
ProcmonIsFileInSystemRoot(
	_In_ PUNICODE_STRING pUniStrImageName
);

BOOLEAN
ProcmonIsFileExist(
	_In_ PUNICODE_STRING pUniStrFileName
);

BOOLEAN
ProcmonAppendVolumeName(
	_In_ PCUNICODE_STRING pUniStrImageName,
	_Inout_ PUNICODE_STRING pUniStrFullName
);

PUNICODE_STRING
FindPagingFileNameInList(
	_In_ PFILE_OBJECT FileObject
);

VOID
AddToPagingFileNameList(
	_In_ PFILE_OBJECT FileObject,
	_In_ PUNICODE_STRING pStrFileName
);

VOID
ProcmonFilePostOptWorkerRoutine(
	_In_ PVOID Parameter
);

PVOID
ProcmonCollectFileOptPostInfo(
	_In_ PETHREAD Thread,
	_In_ UCHAR MajorFunction,
	_In_ FLT_CALLBACK_DATA_FLAGS Flags,
	_In_ PFLT_IO_PARAMETER_BLOCK Iopb,
	_In_ PIO_STATUS_BLOCK IoStatus,
	_In_ PULONG pLength
);

NTSTATUS
ProcmonFilePostOptRoutine(
	_In_ PETHREAD Thread,
	_In_ UCHAR MajorFunction,
	_In_ PIO_STATUS_BLOCK IoStatus,
	_In_opt_ PFLT_IO_PARAMETER_BLOCK Iopb,
	_In_ PVOID CompletionContext,
	_In_ LARGE_INTEGER Time,
	_In_ FLT_CALLBACK_DATA_FLAGS Flags
);