#pragma once

#include <vector>
#include "refobject.hpp"
#include "buffer.hpp"
#include "module.hpp"
#include "viewer.hpp"

class CProcInfo : public CRefBase
{
public:
	CProcInfo(const CString& strPath);
	~CProcInfo();

public:

	const CString& GetDisplayName()
	{
		return m_strDisplay;
	}

	const CString& GetCompanyName()
	{
		return m_strCompanyName;
	}

	const CString& GetVersion()
	{
		return m_strVersion;
	}

	CBuffer& GetSmallIcon()
	{
		return m_SmallIcon;
	}

	CBuffer& GetLargeIcon()
	{
		return m_LargeIcon;
	}

private:

	BOOL Parse(const CString& strPath);

private:

	CString m_strDisplay;
	CString m_strCompanyName;
	CString m_strVersion;
	CBuffer m_SmallIcon;
	CBuffer m_LargeIcon;
};

class CProcess : public CProcInfoView, 
	public CRefBase
{
public:
	CProcess(CRefPtr<CLogEvent> pEvent);
	virtual ~CProcess();

public:
	VOID Dump();
	VOID InsertModule(const CModule& mod);

	VOID SetExitEvent(CRefPtr<CLogEvent> pEvent);

	VOID MarkExit(BOOL bMark)
	{
		m_bMarkExit = bMark;
	}

	BOOL IsMarkExit()
	{
		return m_bMarkExit;
	}

	std::vector<CModule>& GetModuleList();
	CRefPtr<CProcInfo> GetProcInfo();
	CRefPtr<CLogEvent> GetExitEvent();

private:
	BOOL m_bMarkExit = FALSE;
	std::vector<CModule> m_ModuleList;
	CRefPtr<CProcInfo> m_ProcessInfo;
	CRefPtr<CLogEvent> m_ProcessExit;
};