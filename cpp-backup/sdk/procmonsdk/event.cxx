
#include "pch.hpp"
#include "event.hpp"
#include "eventview.hpp"
#include "procmgr.hpp"

#include <assert.h>

USHORT CLogEvent::GetNotifyType()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	PLOG_ENTRY pEntry = (PLOG_ENTRY)m_Prelog.GetBuffer();
	return pEntry->NotifyType;
}

USHORT CLogEvent::GetMoniterType()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->MonitorType;
}


DWORD CLogEvent::GetProcSeq()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->ProcessSeq;
}

DWORD CLogEvent::GetSeq()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->Sequence;
}

CString CLogEvent::GetPath()
{
	return TEXT("");
}

CString CLogEvent::GetDetail()
{
	return TEXT("");
}

CBuffer& CLogEvent::getPreLog()
{
	return m_Prelog;
}

CBuffer& CLogEvent::getPostLog()
{
	return m_Postlog;
}

VOID CLogEvent::setPreLog(const PLOG_ENTRY pEntry)
{
	ULONG EntryLength = CALC_ENTRY_SIZE(pEntry);
	m_Prelog.Write((PBYTE)pEntry, EntryLength);
}

VOID CLogEvent::setPostLog(const PLOG_ENTRY pEntry)
{
	ULONG EntryLength = CALC_ENTRY_SIZE(pEntry);
	m_Postlog.Write((PBYTE)pEntry, EntryLength);
}
