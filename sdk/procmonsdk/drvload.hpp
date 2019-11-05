#pragma once


class CDrvLoader
{
public:
	CDrvLoader();
	virtual ~CDrvLoader();

public:

	BOOL Init(
		IN const CString& strDriverName,
		IN const CString& strDriverPath);

	BOOL Load();
	BOOL UnLoad();
	BOOL IsReady();

private:

	BOOL CreateServiceKey();
	BOOL CreateServiceInstanceKey(HKEY hKey);
	VOID DeleteServiceKey();

private:

	CString m_strDriverName;
	CString m_strDriverPath;
};