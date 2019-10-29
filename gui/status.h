#pragma once

LPCTSTR
StatusGetDesc(
	NTSTATUS Status
);

LPCTSTR
GetClassStringMap(
	int Class
);

LPCTSTR 
GetOperatorStringMap(
	IN PLOG_ENTRY pEntry
);