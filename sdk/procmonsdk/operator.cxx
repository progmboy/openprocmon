
#include "pch.hpp"
#include "operator.hpp"
#include "optview.hpp"
#include "procmgr.hpp"
#include <assert.h>

USHORT COperator::GetNotifyType()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	PLOG_ENTRY pEntry = (PLOG_ENTRY)m_Prelog.GetBuffer();
	return pEntry->NotifyType;
}

USHORT COperator::GetMoniterType()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->MonitorType;
}


DWORD COperator::GetProcSeq()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->ProcessSeq;
}

DWORD COperator::GetSeq()
{
	if (m_Prelog.Empty()) {
		return -1;
	}
	return reinterpret_cast<PLOG_ENTRY>(m_Prelog.GetBuffer())->Sequence;
}

CBuffer& COperator::getPreLog()
{
	return m_Prelog;
}

CBuffer& COperator::getPostLog()
{
	return m_Postlog;
}

VOID COperator::setPreLog(const PLOG_ENTRY pEntry)
{
	ULONG EntryLength = CALC_ENTRY_SIZE(pEntry);
	m_Prelog.Write((PBYTE)pEntry, EntryLength);
}

VOID COperator::setPostLog(const PLOG_ENTRY pEntry)
{
	ULONG EntryLength = CALC_ENTRY_SIZE(pEntry);
	m_Postlog.Write((PBYTE)pEntry, EntryLength);
}
