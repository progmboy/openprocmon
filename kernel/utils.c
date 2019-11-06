
#include "utils.h"
#include <fltKernel.h>


// #ifdef ALLOC_PRAGMA
// #pragma alloc_text(PAGE, ProcmonAllocatePoolWithTag)
// #pragma alloc_text(PAGE, ProcmonDuplicateUnicodeString)
// #pragma alloc_text(PAGE, ProcmonDuplicateUnicodeString2)
// #endif

PVOID
ProcmonAllocatePoolWithTag(
	_In_ POOL_TYPE PoolType,
	_In_ SIZE_T NumberOfBytes,
	_In_ ULONG Tag
)
{
	return ExAllocatePoolWithTag(PoolType, NumberOfBytes, ((CHAR)Tag << 24) | 'nmP');
}

PUNICODE_STRING
ProcmonDuplicateUnicodeString(
	_In_ POOL_TYPE PoolType,
	_In_ CONST PUNICODE_STRING pStrIn,
	_In_ CHAR Tag
)
/*++

Routine Description:

	.

Arguments:

	 PoolType -
	 pStrIn -
	 Tag -

Return Value:

	Routine can return non success error codes.

--*/
{

	FLT_ASSERT(pStrIn);
	FLT_ASSERT(pStrIn->Buffer);

	//
	// Allocate buffer for new string
	//

	PUNICODE_STRING pStrNew = ProcmonAllocatePoolWithTag(PoolType, pStrIn->Length +
		sizeof(UNICODE_STRING), Tag);

	//
	// Initialize the new string and copy the buffer from pStrIn
	//

	if (pStrNew) {
		pStrNew->MaximumLength = pStrIn->Length;
		pStrNew->Buffer = (PWCH)(pStrNew + 1);
		RtlCopyUnicodeString(pStrNew, pStrIn);
	}
	return pStrNew;

}

PWCHAR
ProcmonDuplicateUnicodeString2(
	_Out_ PUNICODE_STRING pDst,
	_In_ PUNICODE_STRING pSrc,
	_In_ ULONG Tag
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
	USHORT wLength;
	PWCHAR lpszRet;

	if (!pSrc->Length) {
		pDst->Length = pDst->MaximumLength = 0;
		pDst->Buffer = NULL;
	}

	wLength = pSrc->Length / sizeof(WCHAR);
	lpszRet = (WCHAR *)ProcmonAllocatePoolWithTag(NonPagedPool, 2 * wLength + 2, Tag);
	pDst->Buffer = lpszRet;
	if (lpszRet)
	{
		memmove(lpszRet, pSrc->Buffer, 2 * wLength);
		lpszRet[wLength] = 0;
		pDst->Length = 2 * wLength;
		pDst->MaximumLength = pDst->Length;
	}
	return lpszRet;
}

USHORT
ProcmonDuplicateUserBuffer(
	_In_ PVOID Src, 
	_In_ USHORT Length, 
	_Out_ PVOID *pDest
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
	USHORT Ret = 0;

	Ret = Length;
	if (Length)
	{
		try
		{
			*pDest = ProcmonAllocatePoolWithTag(NonPagedPool, Length, 'J');
			if (*pDest)
				RtlCopyMemory(*pDest, Src, Length);
			else
				Ret = 0;
		}except(EXCEPTION_EXECUTE_HANDLER){
			if (*pDest){
				ExFreePoolWithTag(*pDest, 0);
				*pDest = NULL;
				return 0;
			}
		}

	}
	return Ret;
}

VOID 
ProcmonSafeCopy(
	_In_ BOOLEAN bIsKernel, 
	_In_ PETHREAD Thread, 
	_In_ FLT_CALLBACK_DATA_FLAGS Flags, 
	_Out_ PVOID pDstBuffer,
	_In_ PVOID pSrcBuffer, 
	_Inout_ PULONG pLength
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
	BOOLEAN bOk = TRUE;
	PMDL pMdl;
	PVOID pMappedAddr;

	if (bIsKernel || Flags & 8 || Flags & 2){
		try{
			RtlCopyMemory(pDstBuffer, pSrcBuffer, *pLength);
		}except(EXCEPTION_EXECUTE_HANDLER){
			bOk = FALSE;
		}
		
	}else{
		pMdl = IoAllocateMdl(pSrcBuffer, *pLength, 0, 0, NULL);
		if (pMdl){
			try{
				MmProbeAndLockProcessPages(pMdl, IoThreadToProcess(Thread), KernelMode, IoReadAccess);
			}except(EXCEPTION_EXECUTE_HANDLER){
				bOk = FALSE;
			}
			
			if (bOk){
				pMappedAddr = MmGetSystemAddressForMdlSafe(pMdl, NormalPagePriority);
				if (pMappedAddr)
					RtlCopyMemory(pDstBuffer, pMappedAddr, *pLength);
				else
					bOk = FALSE;
				MmUnlockPages(pMdl);
			}

			IoFreeMdl(pMdl);
		}else{
			bOk = FALSE;
		}
	}
	if (!bOk){
		*pLength = 0;
		ExFreePoolWithTag(pDstBuffer, 0);
	}
}

PVOID
ObReferenceObjectByHandleSafe(
	_In_ HANDLE Handle
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
	PVOID Object = NULL;

	if (!Handle) {
		return NULL;
	}

	//
	// Like CurrentProcess(-1), CurrentThread(-2)
	//

	if (Handle < 0 && ExGetPreviousMode() == UserMode) {
		return NULL;
	}

	Status = ObReferenceObjectByHandle(Handle, 0, NULL, KernelMode, &Object, NULL);
	if (!NT_SUCCESS(Status)) {
		return NULL;
	}

	return Object;

}

LONG
ProcmonGetFileNameInfoWorkRoutine(
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
	PGETFILENAME_WORKITEM pWorkItem = (PGETFILENAME_WORKITEM)Parameter;
	pWorkItem->Status = FltGetFileNameInformationUnsafe(pWorkItem->FileObject, NULL,
		FLT_FILE_NAME_QUERY_DEFAULT | FLT_FILE_NAME_NORMALIZED,
		&pWorkItem->pFileNameInfo);
	return KeSetEvent(&pWorkItem->NotifyEvent, 0, 0);
}

HANDLE
ProcmonGetProcessTokenHandle(
	_In_ BOOLEAN bRefImpersonationToken
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
	PACCESS_TOKEN pToken;
	HANDLE hToken = NULL;
	BOOLEAN EffectiveOnly, CopyOnOpen;
	SECURITY_IMPERSONATION_LEVEL ImpersonationLevel;

	pToken = PsReferenceImpersonationToken(KeGetCurrentThread(), &CopyOnOpen, &EffectiveOnly, &ImpersonationLevel);
	if (pToken || !bRefImpersonationToken && (pToken = PsReferencePrimaryToken(IoGetCurrentProcess())) != NULL)
	{
		ObOpenObjectByPointer(pToken, OBJ_KERNEL_HANDLE, NULL, TOKEN_QUERY, NULL, KernelMode, &hToken);
		ObfDereferenceObject(pToken);
	}
	return hToken;
}

EXTERN_C
BOOLEAN
NTAPI
PsIsThreadImpersonating(
	__in PETHREAD Thread
);

BOOLEAN
ProcmonIsThreadImpersonation()
/*++

Routine Description:

	.

Arguments:

	 -

Return Value:

	Routine can return non success error codes.

--*/
{
	return PsIsThreadImpersonating(KeGetCurrentThread());
}


PTOKEN_USER
ProcmonQueryTokenInformation(
	_In_ HANDLE hToken,
	_Out_opt_ PTOKEN_STATISTICS pTokenStatistics,
	_Out_opt_ PULONG pTokenVirtualizationEnabled,
	_Out_opt_ PTOKEN_MANDATORY_LABEL *pIntegrityLevel
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
	ULONG Length;
	PTOKEN_USER pTokenUserInfo = NULL;
	CHAR TagTokenInfo = '3';

	Status = ZwQueryInformationToken(hToken, TokenUser, NULL, 0, &Length);
	if (Status != STATUS_BUFFER_TOO_SMALL) {
		return NULL;
	}

	pTokenUserInfo = ProcmonAllocatePoolWithTag(NonPagedPool, Length, TagTokenInfo);
	if (!pTokenUserInfo) {
		return NULL;
	}

	Status = ZwQueryInformationToken(hToken, TokenUser, pTokenUserInfo, Length, &Length);
	if (!NT_SUCCESS(Status)) {
		ExFreePoolWithTag(pTokenUserInfo, 0);
		pTokenUserInfo = NULL;
	}

	if (pTokenStatistics) {
		ZwQueryInformationToken(hToken, TokenStatistics, pTokenStatistics, sizeof(*pTokenStatistics), &Length);
	}

	if (pTokenVirtualizationEnabled) {
		Status = ZwQueryInformationToken(hToken, TokenVirtualizationEnabled, pTokenVirtualizationEnabled,
			sizeof(*pTokenVirtualizationEnabled), &Length);
		if (!NT_SUCCESS(Status)) {
			*pTokenVirtualizationEnabled = (ULONG)-1;
		}
	}

	if (pIntegrityLevel) {

		//
		// Get the length of information
		//

		Status = ZwQueryInformationToken(hToken, TokenIntegrityLevel, NULL, 0, &Length);
		if (Status == STATUS_BUFFER_TOO_SMALL) {

			//
			// Allocate memory for information
			//

			*pIntegrityLevel = (PTOKEN_MANDATORY_LABEL)ProcmonAllocatePoolWithTag(NonPagedPool, Length, TagTokenInfo);
			if (*pIntegrityLevel) {

				//
				// Query again
				//

				Status = ZwQueryInformationToken(hToken, TokenIntegrityLevel, *pIntegrityLevel, Length, &Length);
				if (!NT_SUCCESS(Status)) {
					ExFreePoolWithTag(*pIntegrityLevel, 0);
					*pIntegrityLevel = NULL;
				}
			}
		}
	}

	return pTokenUserInfo;
}

