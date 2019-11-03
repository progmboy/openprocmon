#pragma once

#include "pch.hpp"
#include "process.hpp"
#include "utils.hpp"

VOID CProcess::InsertModule(const CModule& mod)
{
	m_ModuleList.push_back(mod);
}

VOID CProcess::SetExitEvent(CRefPtr<CLogEvent> pEvent)
{
	m_ProcessExit = pEvent;
}

CProcess::CProcess(CRefPtr<CLogEvent> pEvent) :
	CProcInfoView(pEvent)
{
	
	//
	// Create new ProcessInfo
	//
	
	m_ProcessInfo = new CProcInfo(GetImagePath());

}

CProcess::~CProcess()
{
	
}

VOID CProcess::Dump()
{

}

std::vector<CModule>& CProcess::GetModuleList()
{
	return m_ModuleList;
}

CRefPtr<CProcInfo> CProcess::GetProcInfo()
{
	return m_ProcessInfo;
}

CRefPtr<CLogEvent> CProcess::GetExitEvent()
{
	return m_ProcessExit;
}

CProcInfo::CProcInfo(const CString& strPath)
{
	Parse(strPath);
}

CProcInfo::~CProcInfo()
{

}

BOOL CProcInfo::Parse(const CString& strPath)
{
	
	//
	// Query file version info
	//
	
	UtilGetFileVersionInfo(strPath, m_strDisplay, m_strCompanyName, m_strVersion);

	//
	// Query EXE file icon.
	//
	
	UtilExtractIcon(strPath, m_SmallIcon, m_LargeIcon);

	return TRUE;
}
