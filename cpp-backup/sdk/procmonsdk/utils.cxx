
#include "pch.hpp"
#include "utils.hpp"
#include "logger.hpp"
#include <strsafe.h>
#include <winternl.h>
#include <atlstr.h>
#include <atltime.h>
#include <Sddl.h>
#include <map>
#include <minwindef.h>

#pragma comment(lib, "Version.lib")

#define BUFSIZE 512

struct cmpStringNocase {
	bool operator()(const CString& a, const CString& b) const {
		return a.CompareNoCase(b) < 0;
	}
};

std::map<CString, CString> gVolumeCache;

BOOL
CacheVolumePath(
	VOID
)
{
	//
	// Big enough
	//

	TCHAR szTemp[BUFSIZE];
	szTemp[0] = TEXT('\0');

	//
	// Query all volume
	//

	if (GetLogicalDriveStrings(BUFSIZE - 1, szTemp)) {

		TCHAR szName[MAX_PATH];
		TCHAR szDrive[3] = TEXT(" :");
		TCHAR* p = szTemp;


		do
		{
			//
			// Copy the drive letter to the template string
			//

			*szDrive = *p;

			//
			// Look up each device name
			//

			if (QueryDosDevice(szDrive, szName, MAX_PATH)) {
				gVolumeCache.insert(std::make_pair(szName, szDrive));
			}

			//
			// Go to the next NULL character.
			//

			while (*p++);
		} while (*p); // end of string
	}

	return TRUE;
}

BOOL 
UtilConvertNtInternalPathToDosPath(
	IN const CString& strNtPath,
	OUT CString& strDosPath
)
/*++

Routine Description:

	.

Arguments:

	lpszDosPath - Out Dos path the max length is MAX_PATH

Return Value:

	Routine can return non success error codes.

--*/
{
	BOOL bRet = FALSE;
	static BOOL bVolumePathCached = FALSE;
	
	do 
	{
		if (strNtPath.Find(TEXT("\\Device\\LanmanRedirector\\")) == 0) {
			strDosPath += TEXT("\\\\");
			strDosPath += strNtPath.Mid(25);

			bRet = TRUE;
			break;
		}

		if (strNtPath.Find(TEXT("\\Device\\Mup\\")) == 0) {
			strDosPath += TEXT("\\\\");
			strDosPath += strNtPath.Mid(12);
			
			bRet = TRUE;
			break;
		}

		if (strNtPath.Left(12) == TEXT("\\SystemRoot\\")) {
			static TCHAR szSystemRoot[MAX_PATH] = { 0 };

			if (szSystemRoot[0] == 0) {
				GetWindowsDirectory(szSystemRoot, MAX_PATH);
			}

			strDosPath += szSystemRoot;
			if (strDosPath[strDosPath.GetLength() - 1] != TEXT('\\')) {
				strDosPath += TEXT('\\');
			}
			strDosPath += strNtPath.Mid(12);

			bRet = TRUE;
			break;
		}

		if (strNtPath.Left(4) == TEXT("\\??\\")) {
			strDosPath = strNtPath.Mid(4);
			
			bRet = TRUE;
			break;
		}

		if (strNtPath[0] != TEXT('\\')) {
			strDosPath = strNtPath;
			
			bRet = TRUE;
			break;
		}

		if (!bVolumePathCached) {
			CacheVolumePath();
			bVolumePathCached = TRUE;
		}

		for (int i = 0; i < 2; i++)
		{
			for (auto it = gVolumeCache.begin(); it != gVolumeCache.end(); it++)
			{
				CString strDrive = strNtPath.Left(it->first.GetLength());
				if (0 == strDrive.CompareNoCase(it->first)) {
					strDosPath += it->second;
					strDosPath += strNtPath.Mid(it->first.GetLength());
					bRet = TRUE;
					break;
				}
			}

			if (bRet) {
				break;
			}else{

				//
				// CacheVolume and try again
				//

				CacheVolumePath();
			}
		}
	} while (FALSE);
	
	if (!bRet){
		strDosPath.Empty();
	}

	return bRet;
}


BOOL
UtilRegIsLocalMachine(
	IN LPCTSTR lpszRegPath
)
{
	CString strInternal = lpszRegPath;
	CString strStart = TEXT("\\REGISTRY\\MACHINE");

	if (0 != strStart.CompareNoCase(strInternal.Left(strStart.GetLength()))) {
		return FALSE;
	}

	return TRUE;
}

BOOL GetUserSid(CString& strUser)
{
	HANDLE hToken = NULL;
	PTOKEN_USER pUser = NULL;
	LPTSTR lpUserName = NULL;
	BOOL bRet = FALSE;
	DWORD retLength = 0;

	do
	{
		if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &hToken)) {
			return FALSE;
		}

		if (!GetTokenInformation(hToken, TokenUser, NULL, 0, &retLength)) {
			if (GetLastError() != ERROR_INSUFFICIENT_BUFFER) {
				break;
			}
		}

		pUser = (PTOKEN_USER)LocalAlloc(0, retLength);
		if (pUser == NULL) {
			break;
		}

		if (!GetTokenInformation(hToken, TokenUser, pUser, retLength, &retLength)) {
			break;
		}

		if (!ConvertSidToStringSid(pUser->User.Sid, &lpUserName)) {
			break;
		}

		strUser = lpUserName;
		bRet = TRUE;

	} while (FALSE);


	if (hToken) {
		CloseHandle(hToken);
	}

	if (pUser) {
		LocalFree(pUser);
	}

	if (lpUserName) {
		LocalFree(lpUserName);
	}

	return bRet;
}


BOOL
UtilConvertRegInternalToNormal(
	IN const CString& strInternalPath,
	OUT CString& strNormalPath
)
{
	BOOL bRet = FALSE;
	CString strInternalPathTemp = strInternalPath;
	CString strStart = TEXT("\\REGISTRY\\");
	
	//
	// try to get user sid
	//
	
	static CString strUser;
	if (!strUser.GetLength()){
		GetUserSid(strUser);
	}

	if (0 != strStart.CompareNoCase(strInternalPathTemp.Left(strStart.GetLength()))){
		return FALSE;
	}
	
	//
	// Skip \\REGISTRY\\
	//
	
	strInternalPathTemp = strInternalPathTemp.Right(strInternalPathTemp.GetLength() - strStart.GetLength());
	
	//
	// TODO :
	// <INSUFFICIENT RESOURCES>
	// <INVALID NAME>
	//

	do 
	{
		
		//
		// Is start with machine ?
		//
		
		strStart = TEXT("MACHINE");
		if (0 == strStart.CompareNoCase(strInternalPathTemp.Left(strStart.GetLength()))) {

			//
			// Skip MACHINE
			//

			strInternalPathTemp = strInternalPathTemp.Right(strInternalPathTemp.GetLength() - strStart.GetLength());

			strNormalPath += TEXT("HKLM");
			strNormalPath += strInternalPathTemp;

			bRet = TRUE;
			break;
		}

		
		//
		// Is start with user sid?
		//
		
		strStart = TEXT("USER");
		if (0 == strStart.CompareNoCase(strInternalPathTemp.Left(strStart.GetLength()))) {
			
			//
			// Skip USER
			//
			
			strInternalPathTemp = strInternalPathTemp.Right(strInternalPathTemp.GetLength() - strStart.GetLength());
			
			//
			// Skip the '\\'
			//
			strInternalPathTemp.TrimLeft(TEXT('\\'));

			//
			// Start with SID ?
			//
			
			if (0 == strUser.CompareNoCase(strInternalPathTemp.Left(strUser.GetLength()))) {

				strInternalPathTemp = strInternalPathTemp.Right(strInternalPathTemp.GetLength() - strUser.GetLength());

				if (!strInternalPathTemp.IsEmpty() && strInternalPathTemp[0] != TEXT('\\')) {
					CString strClassRoot = TEXT("_Classes");
					if (0 == strClassRoot.CompareNoCase(strInternalPathTemp.Left(strClassRoot.GetLength()))) {
						strNormalPath += TEXT("HKCR");
						strInternalPathTemp = strInternalPathTemp.Right(strInternalPathTemp.GetLength() - strClassRoot.GetLength());
						strNormalPath += strInternalPathTemp;
					}else{
						bRet = TRUE;
						break;
					}
				}else{
					strNormalPath += TEXT("HKCU");
					if(!strInternalPathTemp.IsEmpty()){
						strNormalPath += strInternalPathTemp;
					}
				}

			}else{

				strNormalPath += TEXT("HKU");
				if (!strInternalPathTemp.IsEmpty()) {
					strNormalPath += "\\";
					strNormalPath += strInternalPathTemp;
				}
			}

			bRet = TRUE;
			break;
		}
	} while (FALSE);


	if (!bRet) {
		strNormalPath = strInternalPath;
	}

	return bRet;
}

CString UtilConvertTimeOfDay(LARGE_INTEGER Time)
{
	FILETIME SystemTime;

	SystemTime.dwLowDateTime = Time.LowPart;
	SystemTime.dwHighDateTime = Time.HighPart;
	
	CTime time(SystemTime);
	CString strTimeOfDay;

	strTimeOfDay.Format(TEXT("%02u:%02u:%02u.%07u"),
		time.GetHour(), time.GetMinute(), time.GetSecond(),
		(ULONG)(Time.QuadPart / 10000000));

	return strTimeOfDay;
	
}

CString UtilConvertDay(LARGE_INTEGER Time)
{
	FILETIME SystemTime;

	SystemTime.dwLowDateTime = Time.LowPart;
	SystemTime.dwHighDateTime = Time.HighPart;

	CTime time(SystemTime);

	return time.Format(TEXT("%Y/%m/%d %H:%M:%S"));

}

CString 
UtilConvertTimeSpan(
	LARGE_INTEGER StartTime,
	LARGE_INTEGER CompleteTime
)
{
	FILETIME fStartTime;
	FILETIME fCompleteTime;

	fStartTime.dwLowDateTime = StartTime.LowPart;
	fStartTime.dwHighDateTime = StartTime.HighPart;
	fCompleteTime.dwLowDateTime = CompleteTime.LowPart;
	fCompleteTime.dwHighDateTime = CompleteTime.HighPart;

	CTime timeStart(fStartTime);
	CTime timeFinish(fStartTime);

	CTimeSpan TimeSpan = timeFinish - timeStart;
	CString strTimeOfDay;
	strTimeOfDay.Format(TEXT("%02u:%02u:%02u.%07u"),
		TimeSpan.GetHours(), TimeSpan.GetMinutes(), TimeSpan.GetSeconds(),
		(ULONG)((CompleteTime.QuadPart - StartTime.QuadPart) / 10000000));
	return strTimeOfDay;
	
}

PVOID
VerQueryByCodePage(
	PVOID pVersionInfo,
	WORD wLanguage,
	WORD wCodePage,
	LPCTSTR lpszQuery
)
{
	CString strQueryPath;
	PVOID pQueryRet = NULL;
	UINT cbTranslate = 0;

	strQueryPath.Format(TEXT("\\StringFileInfo\\%04X%04X\\%s"), wLanguage, wCodePage, lpszQuery);

	if(VerQueryValue(pVersionInfo, strQueryPath, &pQueryRet, &cbTranslate)){
		return pQueryRet;
	}
	return NULL;

}

typedef struct _LANGANDCODEPAGE {
	WORD wLanguage;
	WORD wCodePage;
}LANGANDCODEPAGE, *PLANGANDCODEPAGE;

PVOID
VerQueryByTranslation(
	PVOID pVersionInfo,
	LPCTSTR lpszQuery
)
{
	PVOID pValue = NULL;
	UINT cbTranslate = 0;
	PLANGANDCODEPAGE lpTranslate = NULL;
	BOOL bRet;

	bRet = VerQueryValue(pVersionInfo, TEXT("\\VarFileInfo\\Translation"), 
		(LPVOID*)&lpTranslate, &cbTranslate);
	if (bRet && cbTranslate){

		pValue = VerQueryByCodePage(pVersionInfo, lpTranslate->wLanguage, 
			lpTranslate->wCodePage, lpszQuery);

		if (!pValue){
			pValue = VerQueryByCodePage(pVersionInfo,
				lpTranslate->wLanguage, 0x4E4, lpszQuery);
		}
	}

	return pValue;
}

BOOL
UtilGetFileVersionInfo(
	const CString& strFilePath,
	CString& strDescription,
	CString& strCompany,
	CString& strVersion
)
{
	//
	// Get Version 
	//

	BOOL bRet = FALSE;
	PVOID lpVersionInfo = NULL;
	DWORD dwInfoSize;
	DWORD dwHandle;

	dwInfoSize = GetFileVersionInfoSize(strFilePath, &dwHandle);
	if (!dwInfoSize) {
		return FALSE;
	}

	lpVersionInfo = HeapAlloc(GetProcessHeap(), 0, dwInfoSize);
	if (!lpVersionInfo) {
		return FALSE;
	}

	bRet = GetFileVersionInfo(strFilePath, 0, dwInfoSize, lpVersionInfo);
	if (bRet){
		strDescription = (LPCTSTR)VerQueryByTranslation(lpVersionInfo, TEXT("FileDescription"));
		strCompany = (LPCTSTR)VerQueryByTranslation(lpVersionInfo, TEXT("CompanyName"));
		strVersion = (LPCTSTR)VerQueryByTranslation(lpVersionInfo, TEXT("FileVersion"));
	}

	HeapFree(GetProcessHeap(), 0, lpVersionInfo);

	return bRet;
}

typedef struct _EXTRACTICON_PARAM
{
	CBuffer* pBufSmall;
	CBuffer* pBufLarge;
}EXTRACTICON_PARAM, *PEXTRACTICON_PARAM;

BOOL
GetMatchIconBuffer(
	_In_ HMODULE hModule,
	_In_ LPVOID lpIconDir,
	_In_ int cxDesired, 
	_In_ int cyDesired,
	_Out_ CBuffer& clsBuffer
)
{
	DWORD dwResSize;
	PVOID lpResource;
	HGLOBAL hMem;
	HRSRC hResource;

	int nID = LookupIconIdFromDirectoryEx((PBYTE)lpIconDir, TRUE,
		cxDesired, cyDesired, LR_DEFAULTCOLOR);
	if (!nID) {
		return FALSE;
	}

	hResource = FindResource(hModule,
		MAKEINTRESOURCE(nID),
		RT_ICON);

	hMem = LoadResource(hModule, hResource);
	lpResource = LockResource(hMem);
	dwResSize = SizeofResource(hModule, hResource);

	clsBuffer.Clear();
	clsBuffer.Insert((PBYTE)lpResource, dwResSize);

	return TRUE;

}

BOOL
WINAPI
EnumIconResNameProc(
	_In_opt_ HMODULE hModule,
	_In_ LPCWSTR lpType,
	_In_ LPWSTR lpName,
	_In_ LONG_PTR lParam)
{
	
	//
	// 
	//
	
	HGLOBAL hMem;
	LPVOID lpResource;
	PEXTRACTICON_PARAM pParam = (PEXTRACTICON_PARAM)lParam;

	HRSRC hResourceDir = FindResource(hModule, lpName, lpType);
	if (!hResourceDir){
		return TRUE;
	}

	hMem = LoadResource(hModule, hResourceDir);
	lpResource = LockResource(hMem);
	if (!lpResource){
		return TRUE;
	}

	int cxSmall = GetSystemMetrics(SM_CXSMICON);
	int cySmall = GetSystemMetrics(SM_CYSMICON);

	int cxLarge = GetSystemMetrics(SM_CXICON);
	int cyLarge = GetSystemMetrics(SM_CYICON);

	if (!GetMatchIconBuffer(hModule, lpResource, cxSmall, cySmall, *pParam->pBufSmall)){
		return TRUE;
	}

	if (!GetMatchIconBuffer(hModule, lpResource, cxLarge, cyLarge, *pParam->pBufLarge)) {
		return TRUE;
	}

	return FALSE;
}


BOOL 
UtilExtractIcon(
	const CString& strFilePath, 
	CBuffer& bufSmallIcon, 
	CBuffer& bufLargeIcon)
{
	//
	// Try to load the EXE file
	//
	
	HMODULE hExe = LoadLibraryEx(strFilePath, NULL, LOAD_LIBRARY_AS_DATAFILE);
	if (!hExe){
		return FALSE;
	}
	
	EXTRACTICON_PARAM Paramter;
	Paramter.pBufLarge = &bufLargeIcon;
	Paramter.pBufSmall = &bufSmallIcon;

	EnumResourceNames(hExe, RT_GROUP_ICON, EnumIconResNameProc, (LONG_PTR)&Paramter);
	
	if (hExe){
		FreeLibrary(hExe);
	}

	if (!bufSmallIcon.Empty() && !bufLargeIcon.Empty()){
		return TRUE;
	}

	bufSmallIcon.Clear();
	bufLargeIcon.Clear();

	return FALSE;
}

BOOL
UtilAdjustPrivilegesByName(
	IN HANDLE TokenHandle,
	IN LPCTSTR lpName,
	IN BOOL bEnable
)
{
	LUID Luid;
	TOKEN_PRIVILEGES NewState;

	if (LookupPrivilegeValue(NULL, lpName, &Luid)) {
		NewState.Privileges[0].Luid = Luid;
		NewState.PrivilegeCount = 1;
		NewState.Privileges[0].Attributes = bEnable != 0 ? 2 : 0;
		if (AdjustTokenPrivileges(TokenHandle, 0, &NewState, sizeof(TOKEN_PRIVILEGES), NULL, NULL)) {
			return TRUE;
		}
	}
	return FALSE;
}

BOOL UtilSetPriviledge(LPCTSTR lpszPriviledgeName, BOOL bEnable)
{
	HANDLE hToken;
	if (!OpenProcessToken(GetCurrentProcess(), 0xF01FFu, &hToken)) {
		return FALSE;
	}
	if (!UtilAdjustPrivilegesByName(hToken, lpszPriviledgeName, bEnable)) {
		CloseHandle(hToken);
		return FALSE;
	}

	return TRUE;

}