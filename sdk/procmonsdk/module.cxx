
#include "pch.hpp"
#include "module.hpp"
#include "utils.hpp"

CModule::CModule(_In_ PLOG_LOADIMAGE_INFO pInfo)
{
	m_Size = pInfo->ImageSize;
	m_ImageBase = pInfo->ImageBase;

	CString strPath;
	strPath.Append((PWCHAR)(pInfo + 1), pInfo->ImageNameLength);
	UtilConvertNtInternalPathToDosPath(strPath, m_strPath);
}

CModule::CModule()
{

}

CModule::~CModule()
{

}

PVOID CModule::GetImageBase()
{
	return m_ImageBase;
}

ULONG CModule::GetSize()
{
	return m_Size;
}

const CString& CModule::GetPath()
{
	return m_strPath;
}
