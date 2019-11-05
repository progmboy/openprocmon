#pragma once

#include "buffer.hpp"

BOOL
UtilConvertRegInternalToNormal(
	IN const CString& strInternalPath,
	OUT CString& strNormalPath
);

BOOL
UtilConvertNtInternalPathToDosPath(
	IN const CString& strNtPath,
	OUT CString& strDosPath
);

CString UtilConvertTimeOfDay(LARGE_INTEGER Time);
CString UtilConvertDay(LARGE_INTEGER Time);
CString
UtilConvertTimeSpan(
	LARGE_INTEGER StartTime,
	LARGE_INTEGER CompleteTime
);

BOOL
UtilGetFileVersionInfo(
	const CString& strFilePath,
	CString& strDescription,
	CString& strCompany,
	CString& strVersion
);

BOOL
UtilExtractIcon(
	const CString& strFilePath,
	CBuffer& bufSmallIcon,
	CBuffer& bufLargeIcon
);

BOOL 
UtilSetPriviledge(
	IN LPCTSTR lpszPriviledgeName, 
	IN BOOL bEnable
);