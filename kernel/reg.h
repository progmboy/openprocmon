#pragma once

#include <ntifs.h>

typedef struct _REG_POST_INFO
{
	PVOID pRegData;
	PETHREAD Thread;
	LONG Seq;
	// ULONG Fill14;
	LIST_ENTRY List;
}REG_POST_INFO, *PREG_POST_INFO;

typedef struct _REG_OBJECT_INFO
{
	PVOID Object;
	PUNICODE_STRING Name;
	LIST_ENTRY List;
}REG_OBJECT_INFO, *PREG_OBJECT_INFO;

typedef
NTSTATUS
(NTAPI *FNCmRegisterCallback)(
	_In_     PEX_CALLBACK_FUNCTION Function,
	_In_opt_ PVOID                 Context,
	_Out_    PLARGE_INTEGER        Cookie
	);

typedef
NTSTATUS
(NTAPI *FNCmRegisterCallbackEx)(
	_In_        PEX_CALLBACK_FUNCTION   Function,
	_In_        PCUNICODE_STRING        Altitude,
	_In_        PVOID                   Driver, //PDRIVER_OBJECT
	_In_opt_    PVOID                   Context,
	_Out_       PLARGE_INTEGER          Cookie,
	_Reserved_  PVOID                   Reserved
	);

typedef
NTSTATUS
(NTAPI *FNCmUnRegisterCallback)(
	_In_ LARGE_INTEGER    Cookie
	);

typedef
NTSTATUS
(NTAPI *FNCmCallbackGetKeyObjectID)(
	_In_            PLARGE_INTEGER      Cookie,
	_In_            PVOID               Object,
	_Out_opt_       PULONG_PTR          ObjectID,
	_Outptr_opt_ PCUNICODE_STRING    *ObjectName
	);

VOID
ProcmonRegMonitorInit(
	VOID
);

NTSTATUS
EnableRegMonitor(
	_In_ ULONG bEnable
);