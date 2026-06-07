
#include "pch.hpp"
#include "logger.hpp"
#include "process.hpp"
#include "eventview.hpp"
#include "procmgr.hpp"
#include "eventmgr.hpp"
#include "procopt.hpp"
#include "eventfactory.hpp"

#include <conio.h>

CEventMgr::CEventMgr()
{
	RegisterProcessor(new CProcOpt);
}

BOOL
CEventMgr::ProcessMsgBlocks(
	_In_ PLOG_ENTRY pEntries,
	_In_ ULONG Length)
{
	ULONG TotalLength = 0;
	PLOG_ENTRY pEntry;
	pEntry = pEntries;
	if (!Length || !pEntries || Length < sizeof(LOG_ENTRY)){
		return FALSE;
	}

	while (TRUE)
	{

		//
		// Process Entry
		//


		ProcessEntry(pEntry);

		//
		// calc entry length
		//

		ULONG EntryLength = CALC_ENTRY_SIZE(pEntry);

		TotalLength += EntryLength;
		if (TotalLength >= Length) {
			break;
		}

		//
		// Next Entry
		//

		pEntry = (PLOG_ENTRY)((ULONG_PTR)pEntry + EntryLength);
	}

	return TRUE;
}

BOOL
CEventMgr::ProcessEntry(
	_In_ const PLOG_ENTRY pEntry
)
{
	//
	// filter the type
	//

	if (pEntry->MonitorType > MONITOR_TYPE_PROFILING ||
		pEntry->MonitorType < MONITOR_TYPE_POST) {
		return FALSE;
	}

	//
	// TODO: support thread profiling
	//

	if (pEntry->MonitorType == MONITOR_TYPE_PROFILING) {
		return TRUE;
	}

	//
	// Filter ourself
	//

	if (pEntry->MonitorType == MONITOR_TYPE_POST) {


		//
		// find the pre-operator
		//

		auto pEvent = RefEvent(pEntry->Sequence);
		if (pEvent.IsNull()) {
			return FALSE;
		}

		//
		// Set the post log
		//

		pEvent->setPostLog(pEntry);

		//
		// this operator is finish add to queue
		//

		PushEvent(pEvent);

		//
		// 将这个Operator在列表中移除
		//

		RemoveFromList(pEntry->Sequence);

	}else{

		//
		// If Status == STATUS_PENDING
		// is will has POST opt
		//

		CRefPtr<CLogEvent> newEvent = CEventFactory::CreateInstance(pEntry->MonitorType);
		newEvent->setPreLog(pEntry);

		if (pEntry->Status == STATUS_PENDING) {

			//
			// Insert to map
			//

			InsertOperator(pEntry->Sequence, newEvent);

		}else{
			PushEvent(newEvent);
		}
	}

	return TRUE;
}

VOID CEventMgr::InsertOperator(ULONG Seq, CRefPtr<CLogEvent> pEvent)
{
	m_EventMap.insert(std::make_pair(Seq, pEvent));
}

CRefPtr<CLogEvent> CEventMgr::RefEvent(ULONG Seq)
{
	EVENT_MAP::iterator it;
	it = m_EventMap.find(Seq);
	if (it != m_EventMap.end()) {
		return it->second;
	}

	return NULL;
}

VOID CEventMgr::RemoveFromList(ULONG Seq)
{
	CRefPtr<CLogEvent> OptFind = RefEvent(Seq);
	if (!OptFind.IsNull()) {
		m_EventMap.erase(Seq);
	}else{
		LogMessage(L_INFO, TEXT("Remove Opt seq 0x%x is not exist in list"), Seq);
		//__debugbreak();
		//_getch();
	}
}

BOOL CEventMgr::Process()
{

	//
	// Get one from Operator Queue
	//

	CRefPtr<CLogEvent> pEvent = PopEvent();
	if (pEvent.IsNull()) {
		return FALSE;
	}

	for (auto it = m_Processors.begin(); it != m_Processors.end(); it++)
	{
		PLOG_ENTRY pPreEntry = (PLOG_ENTRY)pEvent->getPreLog().GetBuffer();
		if ((*it)->IsType(pPreEntry->MonitorType)) {

			//
			// pre process
			//

			(*it)->Process(pEvent);
			break;
		}
	}

	//
	// gen view
	//

	CRefPtr<CProcess> pProcess = PROCMGR().RefProcessBySeq(pEvent->GetProcSeq());
	if (!pProcess.IsNull()) {

		CRefPtr <CEventView> pView = new CEventView;
		pView->SetEventOpt(pEvent);
		pView->SnapProcess(pProcess);

		//
		// do callback
		//

		for (auto itcall = m_callBackList.begin(); itcall != m_callBackList.end(); itcall++)
		{
			(*itcall)->DoEvent(pView);
		}
	}


	return TRUE;

}

VOID CEventMgr::RegisterProcessor(CRefPtr<IProcessor> Processor)
{
	m_Processors.push_back(Processor);
}

VOID CEventMgr::RegisterCallback(CRefPtr<IEventCallback> pCallback)
{
	m_callBackList.push_back(pCallback);
}

VOID CEventMgr::Clear()
{
	m_lock.lock();
	while (!m_msgQueue.empty()) {
		m_msgQueue.pop();
	}
	m_lock.unlock();

	m_EventMap.clear();
	m_PushCount = 0;
	m_PopCount = 0;
}

VOID CEventMgr::PushEvent(CRefPtr<CLogEvent> pEvent)
{
	std::unique_lock<std::mutex> lck(m_lock);
	m_msgQueue.push(pEvent);

	//
	// Notify process thread the log data coming
	//

	m_PushCount++;
	m_convar.notify_all();
}

CRefPtr<CLogEvent> CEventMgr::PopEvent()
{
	CRefPtr<CLogEvent> pEvent;
	std::unique_lock<std::mutex> lck(m_lock);

	//
	// wait until data coming
	//

	while (TRUE)
	{
		if (m_msgQueue.empty()) {
			if (m_convar.wait_for(lck, std::chrono::milliseconds(500)) == std::cv_status::no_timeout) {
				
				//
				// here some event coming
				// but here will be some fake signal 
				//
				
 				if (m_msgQueue.empty()){
 					continue;
 				}else{
 					break;
 				}

			}else{
				
				//
				// here wait time out we just return NULL 
				// to give a chance to loop to check is the thread what to exist
				//
				
				return NULL;
			}
		}else{
			break;
		}
	}

	m_PopCount++;

	//
	// get the front of the queue
	//

	pEvent = m_msgQueue.front();
	m_msgQueue.pop();

	return pEvent;
}
