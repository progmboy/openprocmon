#pragma once

LPCTSTR
StrMapNtStatus(
	_In_ NTSTATUS Status
);

CString
StrMapSecurityInformation(
	_In_ DWORD dwSecurityInformation
);

CString
StrMapFileAccessMask(
	_In_ DWORD AccessMask
);

LPCTSTR 
StrMapFileCreateDisposition(
	_In_ DWORD CreateDisposition
);

LPCTSTR 
StrMapFileRetDisposition(
	_In_ DWORD CreateDisposition
);

CString
StrMapFileCreateOptions(
	_In_ DWORD CreateOptions
);

CString 
StrMapFileAttributes(
	_In_ DWORD FileAttributes
);

CString
StrMapFileShareAccess(
	_In_ DWORD ShareAccess
);

LPCTSTR
StrMapClassEvent(
	_In_ int Class
);

LPCTSTR 
StrMapOperation(
	_In_ PLOG_ENTRY pEntry
);

CString
StrMapUserNameFromSid(
	_In_ PSID pSid
);

LPCTSTR
StrMapIntegrityLevel(
	_In_ DWORD dwIntegrityLevel
);