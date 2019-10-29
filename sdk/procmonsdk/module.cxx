
#include "pch.hpp"
#include "module.hpp"

CModule::CModule(_In_ PLOG_LOADIMAGE_INFO pInfo)
{
	m_Size = pInfo->ImageSize;
	m_ImageBase = pInfo->ImageBase;
	m_strPath.Append((PWCHAR)(pInfo + 1), pInfo->ImageNameLength);
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
