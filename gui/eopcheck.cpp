
#include "stdafx.h"
#include <assert.h>
#include "eopcheck.hpp"
#include <sddl.h>

#define FILE_WRITE_ACESS_MASK      (FILE_WRITE_DATA          |\
                                    FILE_WRITE_ATTRIBUTES    |\
                                    FILE_WRITE_EA            |\
                                    FILE_APPEND_DATA	     |\
									DELETE)

#define IS_FILE_ACCESSMASK_HAS_WRITE(_accessmask) (_accessmask == GENERIC_WRITE || (_accessmask & FILE_WRITE_ACESS_MASK))

#define IS_DIRECTORY_ACCESSMASK_HAS_WRITE(_accessmask) (_accessmask == GENERIC_WRITE || \
				(_accessmask == FILE_GENERIC_WRITE) || \
				((_accessmask & FILE_WRITE_ACESS_MASK) == FILE_WRITE_ACESS_MASK) || \
				(_accessmask & WRITE_DAC))


#define IS_FILE_CRATE_OPTIONS_HAS_DELETE(_Options) ((_Options & 0x00FFFFFF) & FILE_DELETE_ON_CLOSE)


CEopCheck::CEopCheck()
{

}

CEopCheck::~CEopCheck()
{
	if (m_hDesktopProcess){
		CloseHandle(m_hDesktopProcess);
	}
}

BOOL
CEopCheck::IsFileWriteAccessFromCache(
	IN const CString& lpszFilePath,
	OUT PDWORD GrantedAccess
)
{
	std::map<CString, DWORD>::iterator it;

	it = m_FileWriteAccessMap.find(lpszFilePath);
	if (it != m_FileWriteAccessMap.end()) {
		*GrantedAccess = it->second;
		return TRUE;
	}

	return FALSE;
}

HANDLE
CEopCheck::OpenDesktopProcessToken(
	VOID
)
{
	BOOL bRet = FALSE;
	HWND hWnd = GetShellWindow();
	DWORD dwPID;
	HANDLE hShellProcess = NULL;
	HANDLE hShellProcessToken = NULL;
	HANDLE hTokenRet = NULL;

	GetWindowThreadProcessId(hWnd, &dwPID);
	if (0 == dwPID) {
		return NULL;
	}

	//
	// Open the desktop shell process in order to query it (get the token)
	//

	hShellProcess = OpenProcess(PROCESS_QUERY_INFORMATION, FALSE, dwPID);
	if (!hShellProcess) {
		return NULL;
	}

	//
	// Get the process token of the desktop shell.
	//

	bRet = OpenProcessToken(hShellProcess, TOKEN_DUPLICATE | TOKEN_IMPERSONATE | TOKEN_QUERY, &hShellProcessToken);
	if (!bRet) {
		goto cleanup;
	}

	//
	// Duplicate the shell's process token to get a primary token.
	// Based on experimentation, this is the minimal set of rights required for
	// CreateProcessWithTokenW (contrary to current documentation).
	//

	bRet = DuplicateTokenEx(hShellProcessToken, 0, NULL,
		SecurityImpersonation, TokenImpersonation, &hTokenRet);


cleanup:

	//
	// Clean up resources
	//

	if (hShellProcessToken) {
		CloseHandle(hShellProcessToken);
	}

	if (hShellProcess) {
		CloseHandle(hShellProcess);
	}

	return hTokenRet;
}

HANDLE
CEopCheck::RefDesktopProcessToken(
	VOID
)
{
	if (!m_hDesktopProcess){
		m_hDesktopProcess = OpenDesktopProcessToken();
	}
	return m_hDesktopProcess;
}

BOOL
CEopCheck::IsFileWritableByMeduimProcess(
	IN const CString& lpszFilePath
)
{
	BOOL bAccess = FALSE;
	DWORD GrantedAccess = 0;

	if(GetFileGrantedAccessByMeduimProcess(lpszFilePath, &GrantedAccess)){
		if (IS_FILE_ACCESSMASK_HAS_WRITE(GrantedAccess)) {
			bAccess = TRUE;
		}
	}

	return bAccess;
}

BOOL
CEopCheck::IsFileDirWritableByMeduimProcess(
	IN const CString& lpszFilePath
)
{
	BOOL bAccess = FALSE;
	DWORD GrantedAccess = 0;

	CString strDir = GetFileDirectory(lpszFilePath);

	if (GetFileGrantedAccessByMeduimProcess(strDir, &GrantedAccess)) {
		if (IS_DIRECTORY_ACCESSMASK_HAS_WRITE(GrantedAccess)) {
			bAccess = TRUE;
		}
	}

	return bAccess;
}

BOOL
CEopCheck::GetFileGrantedAccessByMeduimProcess(
	IN const CString& lpszFilePath,
	OUT PDWORD pGrantedAccess
)
{
	BOOL bRet, bAccess = FALSE;
	DWORD dwLengthNeeded;
	PSECURITY_DESCRIPTOR pSecurityDescriptor = NULL;
	PRIVILEGE_SET PrivilegeSet;
	DWORD GrantedAccess, PrivilegeSetLength;
	BOOL AccessStatus;

	GENERIC_MAPPING Mapping = {
		//FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_GENERIC_EXECUTE, FILE_ALL_ACCESS
		0x120089, 0x120116, 0x1200A0, 0x1F03FF
	};


	//
	// Access from cache first
	//

	if (IsFileWriteAccessFromCache(lpszFilePath, &GrantedAccess)) {
		if (pGrantedAccess){
			*pGrantedAccess = GrantedAccess;
		}
		
		return TRUE;
	}

	//
	// Get explorer token
	//

	HANDLE hToken = RefDesktopProcessToken();


	bRet = GetFileSecurity(lpszFilePath, LABEL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION |
		GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, NULL, 0, &dwLengthNeeded);
	DWORD dwError = GetLastError();
	if (!bRet && dwError == ERROR_INSUFFICIENT_BUFFER) {
		pSecurityDescriptor = (PSECURITY_DESCRIPTOR)LocalAlloc(0, dwLengthNeeded);
		if (!pSecurityDescriptor){
			return FALSE;
		}
		bRet = GetFileSecurity(lpszFilePath, LABEL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION |
			GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, pSecurityDescriptor,
			dwLengthNeeded, &dwLengthNeeded);

		PrivilegeSetLength = sizeof(PrivilegeSet);
		bRet = AccessCheck(pSecurityDescriptor, hToken,
			MAXIMUM_ALLOWED, &Mapping, &PrivilegeSet,
			&PrivilegeSetLength, &GrantedAccess, &AccessStatus);
		if (bRet) {

			if (pGrantedAccess){
				*pGrantedAccess = GrantedAccess;
			}

			//
			// Add to cache
			//

			m_FileWriteAccessMap.insert(std::make_pair(lpszFilePath, GrantedAccess));
		}

		LocalFree(pSecurityDescriptor);
	}

	return bRet;
}

CString CEopCheck::GetFileDirectory(const CString& strFile)
{
	TCHAR szPath[MAX_PATH];

	CString strPath = strFile;
	strPath.TrimRight(TEXT('\\'));
	StringCchCopy(szPath, MAX_PATH, strPath);
	PathRemoveFileSpec(szPath);

	return szPath;

}

BOOL CEopCheck::Check(CRefPtr<CEventView> pEvent, BOOL& bHighlight)
{
	
	//
	// First we check the process integrity level
	// we only care about the process which is system or high level 
	//
	
	BOOL bIsEopBug = FALSE;
	DWORD dwEventClass = pEvent->GetEventClass();
	DWORD dwNotifyType = pEvent->GetEventOperator();
	PVOID pPreEventEntry = pEvent->GetPreEventEntry();
	PVOID pPostEventEntry = pEvent->GetPostEventEntry();
	BOOL IsImpersonateOpen = pEvent->IsImpersonateOpen();
	BOOL IsImpersonate = pEvent->IsImpersonate();

	if (pEvent->GetProcessId() == GetCurrentProcessId()){
		return FALSE;
	}

	if (pEvent->GetIntegrity() < SECURITY_MANDATORY_HIGH_RID){
		return FALSE;
	}

	if (pEvent->GetResult() < 0) {
		return FALSE;
	}

	if (dwEventClass == MONITOR_TYPE_FILE){

		CString strPath = pEvent->GetPath();
		dwNotifyType = (UCHAR)dwNotifyType - 20;

		if (strPath.IsEmpty() || IsFilter(strPath)){
			return FALSE;
		}

		PLOG_FILE_OPT pFileOpt = TO_EVENT_DATA(PLOG_FILE_OPT, pPreEventEntry);

		switch (dwNotifyType)
		{
		case IRP_MJ_CREATE:
			{
				PLOG_FILE_CREATE pCreateInfo = reinterpret_cast<PLOG_FILE_CREATE>(pFileOpt->Name + pFileOpt->NameLength);
				ULONG_PTR* pInformation = TO_EVENT_DATA(ULONG_PTR*, pPostEventEntry);
				PUSHORT pSetSecInfo = (PUSHORT)((PUCHAR)(pCreateInfo + 1) + pCreateInfo->UserTokenLength);

				if (IsImpersonateOpen && IsImpersonate) {
					return FALSE;
				}

				BOOL bDeleteFile = IS_FILE_CRATE_OPTIONS_HAS_DELETE(pFileOpt->FltParameter.Create.Options);

				if (*pInformation == FILE_CREATED ||
					IS_FILE_ACCESSMASK_HAS_WRITE(pCreateInfo->DesiredAccess) ||
					bDeleteFile) {

					if (PathFileExists(strPath)) {
						if (IsFileWritableByMeduimProcess(strPath) &&
							IsFileDirWritableByMeduimProcess(strPath)) {
							bIsEopBug = TRUE;
						}
					}else{
						if(IsFileDirWritableByMeduimProcess(strPath)){
							bIsEopBug = TRUE;
						}
					}

					if (bIsEopBug) {
						if (*pSetSecInfo) {
							bHighlight = TRUE;
						}
					}

					if (bDeleteFile) {
						bHighlight = TRUE;
					}
				}
			}
			break;
		//case IRP_MJ_SET_INFORMATION:
		case IRP_MJ_SET_SECURITY:
		{
			if (IsImpersonateOpen && IsImpersonate) {
				return FALSE;
			}

			if (IsFileWritableByMeduimProcess(strPath) &&
				IsFileDirWritableByMeduimProcess(strPath)) {
				bIsEopBug = TRUE;
				bHighlight = TRUE;
			}

			break;
		}
		default:
			break;
		}
	}

	return bIsEopBug;

}

const TCHAR* gFilterList[] = {
	TEXT("C:"),
	TEXT("D:"),
	TEXT("E:"),
	TEXT("C:\\"),
	TEXT("C:\\Pagefile.sys"),
	TEXT("*nvstapisvr.log"),
};

BOOL
CEopCheck::IsFilter(
	IN const CString& strPath
)
{
	CPath cPath(strPath);

	for (int i = 0; i < _countof(gFilterList); i++)
	{
		if (cPath.MatchSpec(gFilterList[i])) {
			return TRUE;
		}
	}

	return FALSE;
}
