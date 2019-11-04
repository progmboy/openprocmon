#pragma once


class CModule
{
public:
	CModule();
	CModule(_In_ PLOG_LOADIMAGE_INFO pInfo);
	~CModule();

public:

	PVOID GetImageBase();
	ULONG GetSize();
	const CString& GetPath();
	BOOL IsAddressIn(LPVOID lpAddress);

private:
	PVOID m_ImageBase = NULL;
	ULONG m_Size = 0;
	CString m_strPath;
};