#pragma once

#include <ntifs.h>
#include <fltKernel.h>


typedef struct _GETFILENAME_WORKITEM
{
	WORK_QUEUE_ITEM WorkItem;
	NTSTATUS Status;
	PFILE_OBJECT FileObject;
	PFLT_FILE_NAME_INFORMATION pFileNameInfo;
	KEVENT NotifyEvent;
}GETFILENAME_WORKITEM, *PGETFILENAME_WORKITEM;

PVOID
ProcmonAllocatePoolWithTag(
	_In_ POOL_TYPE PoolType,
	_In_ SIZE_T NumberOfBytes,
	_In_ ULONG Tag
);

PUNICODE_STRING
ProcmonDuplicateUnicodeString(
	_In_ POOL_TYPE PoolType,
	_In_ CONST PUNICODE_STRING pStrIn,
	_In_ CHAR Tag
);

PWCHAR
ProcmonDuplicateUnicodeString2(
	_Out_ PUNICODE_STRING pDst,
	_In_ PUNICODE_STRING pSrc,
	_In_ ULONG Tag
);

USHORT
ProcmonDuplicateUserBuffer(
	_In_ PVOID Src,
	_In_ USHORT Length,
	_Out_ PVOID *pDest
);

PVOID
ObReferenceObjectByHandleSafe(
	_In_ HANDLE Handle
);

LONG
ProcmonGetFileNameInfoWorkRoutine(
	_In_ PVOID Parameter
);

HANDLE
ProcmonGetProcessTokenHandle(
	_In_ BOOLEAN bRefImpersonationToken
);

PTOKEN_USER
ProcmonQueryTokenInformation(
	_In_ HANDLE hToken,
	_Out_opt_ PTOKEN_STATISTICS pTokenStatistics,
	_Out_opt_ PULONG pTokenVirtualizationEnabled,
	_Out_opt_ PTOKEN_MANDATORY_LABEL *pIntegrityLevel
);

VOID
ProcmonSafeCopy(
	_In_ BOOLEAN bIsKernel,
	_In_ PETHREAD Thread,
	_In_ FLT_CALLBACK_DATA_FLAGS Flags,
	_Out_ PVOID pDstBuffer,
	_In_ PVOID pSrcBuffer,
	_Inout_ PULONG pLength
);

BOOLEAN
ProcmonIsThreadImpersonation();