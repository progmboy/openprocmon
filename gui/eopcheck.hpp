#pragma once

struct cmpStringNocase {
	bool operator()(const CString& a, const CString& b) const {
		return a.CompareNoCase(b) < 0;
	}
};

class CEopCheck
{
public:
	CEopCheck();
	~CEopCheck();

	BOOL Check(CRefPtr<CEventView> pEvent);

private:

	BOOL IsFileWriteAccessFromCache(
		IN const CString& lpszFilePath,
		OUT PDWORD GrantedAccess);
	HANDLE OpenDesktopProcessToken(VOID);
	HANDLE RefDesktopProcessToken(VOID);
	BOOL IsFileWritableByMeduimProcess(IN const CString& lpszFilePath);
	BOOL IsFileDirWritableByMeduimProcess(IN const CString& lpszFilePath);
	BOOL GetFileGrantedAccessByMeduimProcess(IN const CString& lpszFilePath, OUT PDWORD pGrantedAccess);
	CString GetFileDirectory(const CString& strFile);
	BOOL IsFilter(IN const CString& strPath);
	
private:

	HANDLE m_hDesktopProcess = NULL;
	std::map<CString, DWORD, cmpStringNocase> m_FileWriteAccessMap;
};
