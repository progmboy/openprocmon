#pragma once

LPCTSTR
StrMapNtStatus(
	_In_ NTSTATUS Status
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