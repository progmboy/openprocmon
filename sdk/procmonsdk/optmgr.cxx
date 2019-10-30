
#include "pch.hpp"
#include "process.hpp"
#include "procmgr.hpp"
#include "optmgr.hpp"

#include "procopt.hpp"
#include "fileopt.hpp"

#include <conio.h>

COperatorMgr::COperatorMgr()
{
	RegisterProcessor(new CProcOpt);
	RegisterProcessor(new CFileOpt);
}

BOOL
COperatorMgr::ProcessMsgBlocks(
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
COperatorMgr::ProcessEntry(
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

	//CRefPtr<CProcess> Process = RefProcess(pEntry->ProcessSeq);
	//if (!Process.IsNull() && Process->GetProcessId() == GetCurrentProcessId()) {
	//	return FALSE;
	//}

	if (pEntry->MonitorType == MONITOR_TYPE_POST) {


		//
		// find the pre-operator
		//

		auto Operator = RefOperator(pEntry->Sequence);
		if (Operator.IsNull()) {
			return FALSE;
		}

		//PLOG_ENTRY pPreEntry = (PLOG_ENTRY)Operator->getPreLog().GetBuffer();
		//LogMessage(L_INFO, TEXT("pEntry = 0x%p preEntry =0x%p MonitorType=0x%x event=0x%x seq=0x%x"), 
		//	pEntry, pPreEntry, (ULONG)pPreEntry->MonitorType, (ULONG)pPreEntry->NotifyType, pPreEntry->Sequence);

		//
		// Set the post log
		//

		Operator->setPostLog(pEntry);

		//
		// this operator is finish add to queue
		//

		PushOpt(Operator);

		//
		// 将这个Operator在列表中移除
		//

		RemoveFromList(pEntry->Sequence);

	}else{

		//
		// If Status == STATUS_PENDING
		// is will has POST opt
		//

		CRefPtr<COperator> newOperator = new COperator;
		newOperator->setPreLog(pEntry);

		if (pEntry->Status == STATUS_PENDING) {

			//
			// Insert to map
			//

			m_OperatorMap.insert(std::make_pair(pEntry->Sequence, newOperator));

		}else{
			PushOpt(newOperator);
		}
	}

	return TRUE;
}

CRefPtr<COperator> COperatorMgr::RefOperator(ULONG Seq)
{
	OPERATOR_MAP::iterator it;
	it = m_OperatorMap.find(Seq);
	if (it != m_OperatorMap.end()) {
		return it->second;
	}

	return NULL;
}

VOID COperatorMgr::RemoveFromList(ULONG Seq)
{
	CRefPtr<COperator> OptFind = RefOperator(Seq);
	if (!OptFind.IsNull()) {
		m_OperatorMap.erase(Seq);
	}else{
		LogMessage(L_INFO, TEXT("Remove Opt seq 0x%x is not exist in list"), Seq);
		//__debugbreak();
		//_getch();
	}
}

BOOL COperatorMgr::Process()
{

	//
	// Get one from Operator Queue
	//

	CRefPtr<COperator> Operator = PopOpt();
	if (Operator.IsNull()) {
		return FALSE;
	}

	//LogMessage(L_INFO, TEXT("Pop event 0x%x"), Operator->GetSeq());

	for (auto it = m_Processors.begin(); it != m_Processors.end(); it++)
	{
		PLOG_ENTRY pPreEntry = (PLOG_ENTRY)Operator->getPreLog().GetBuffer();
		if ((*it)->IsType(pPreEntry->MonitorType)) {

			//
			// pre process
			//

			(*it)->Process(Operator);

			//
			// Parse the operator
			//

			if ((*it)->Parse(Operator)) {

				//
				// gen view
				//

				CRefPtr<CProcess> pProcess = PROCMGR().RefProcessBySeq(Operator->GetProcSeq());
				if (!pProcess.IsNull()) {

					CRefPtr <COptView> pView = new COptView;
					pView->SetEventOpt(Operator);
					pView->SnapProcess(pProcess);

					//
					// do callback
					//

					for (auto itcall = m_callBackList.begin(); itcall != m_callBackList.end(); itcall++)
					{
						(*itcall)->DoEvent(pView);
					}
				}

			}

		}
	}


	return TRUE;

}

VOID COperatorMgr::RegisterProcessor(CRefPtr<IProcessor> Processor)
{
	m_Processors.push_back(Processor);
}

VOID COperatorMgr::RegisterCallback(CRefPtr<IEventCallback> pCallback)
{
	m_callBackList.push_back(pCallback);
}

VOID COperatorMgr::Clear()
{
	m_lock.lock();
	while (!m_msgQueue.empty()) {
		m_msgQueue.pop();
	}
	m_lock.unlock();

	m_OperatorMap.clear();
	m_PushCount = 0;
	m_PopCount = 0;
}

VOID COperatorMgr::PushOpt(CRefPtr<COperator> opt)
{
	std::unique_lock<std::mutex> lck(m_lock);
	m_msgQueue.push(opt);

	//
	// Notify process thread the log data coming
	//

	//LogMessage(L_INFO, TEXT("Queue %d not process"), m_PushCount - m_PopCount);

	m_PushCount++;
	m_convar.notify_all();
}

CRefPtr<COperator> COperatorMgr::PopOpt()
{
	CRefPtr<COperator> Operator;
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

	Operator = m_msgQueue.front();
	m_msgQueue.pop();

	return Operator;
}
