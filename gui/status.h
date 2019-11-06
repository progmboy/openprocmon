#pragma once

typedef enum _MAP_SOURCE_TYPE
{
	emArchiteture,
	emAuthId,
	emCategory,
	emCommandLine,
	emCompany,
	emCompletionTime,
	emDataTime,
	emDescription,
	emDetail,
	emDuration,
	emEventClass,
	emImagePath,
	emIntegrity,
	emOperation,
	emParentPid,
	emPath,
	emPID,
	emProcessName,
	emRelativeTime,
	emResult,
	emSequence,
	emSession,
	emTID,
	emTimeOfDay,
	emUser,
	emVersion,
	emVirtualize
}MAP_SOURCE_TYPE;

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

CString
MapMonitorResult(
	MAP_SOURCE_TYPE SrcType,
	const CRefPtr<CEventView> pOptView);