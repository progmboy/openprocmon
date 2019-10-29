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

	BOOL EnablePrivilege();
	BOOL Load();
	BOOL UnLoad();

private:

	BOOL CreateServiceKey();
	VOID DeleteServiceKey();
	BOOL IsReady();

private:

	CString m_strDriverName;
	CString m_strDriverPath;
};