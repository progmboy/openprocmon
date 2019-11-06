
#include "pch.hpp"
#include "procmgr.hpp"
#include "logger.hpp"

CProcMgr::CProcMgr()
{

}

CProcMgr::~CProcMgr()
{

}

CRefPtr<CProcess> 
CProcMgr::RefProcessBySeq(
	_In_ ULONG Seq
)
{
	//
	// aquire read lock
	//

	std::shared_lock<std::shared_mutex> lock(m_lock);
	auto it = m_ProcessList.find(Seq);
	if (it == m_ProcessList.end()) {
		return NULL;
	}

	return it->second;
}

CRefPtr<CProcess> 
CProcMgr::RefProcessByProcessId(
	_In_ ULONG ProcessId
)
{
	
	//
	// this will be very slowly
	// Do not use this method if is necessary
	//
	
	//
	// aquire read lock
	//

	std::shared_lock<std::shared_mutex> lock(m_lock);

	for (auto it = m_ProcessList.begin(); it != m_ProcessList.end(); it++)
	{
		CRefPtr<CProcess> Process = it->second;
		if (Process->GetProcessId() == ProcessId){
			return Process;
		}
	}
	
	return NULL;
}

VOID CProcMgr::InsertModule(
	_In_ ULONG ProcSeq,
	_In_ PLOG_LOADIMAGE_INFO pInfo
)
{
	CRefPtr<CProcess> Process = RefProcessBySeq(ProcSeq);
	if (!Process.IsNull()){
		CModule mod(pInfo);
		Process->InsertModule(mod);
	}
}

VOID
CProcMgr::Replace(
	_In_ ULONG Seq,
	_In_ CRefPtr<CProcess> Process
)
{
	std::unique_lock<std::shared_mutex> lock(m_lock);

	//
	// remove from list
	//

	m_ProcessList.erase(Seq);

	//
	// Replace it with new one
	//

	m_ProcessList.insert(PROCESSLISTMAPPAIR(Seq, Process));
}

VOID 
CProcMgr::Insert(
	_In_ CRefPtr<CProcess> Process
)
{

	//
	// Check is process already in list
	//

	ULONG Seq = Process->GetProcSeq();
	CRefPtr<CProcess> ProcessFind = RefProcessBySeq(Seq);

	if (ProcessFind.IsNull()) {

		//
		// Add to process list
		//

		m_lock.lock();
		m_ProcessList.insert(PROCESSLISTMAPPAIR(Seq, Process));
		m_lock.unlock();
	}else{
		
		//
		// Replace it anyway
		//
		
		Replace(Seq, Process);

		if (!ProcessFind->IsMarkExit()) {

			//
			// Here must be something wrong
			//
			
			if (!Process->GetProcessId()) {
				Process->Dump();
			}

			LogMessage(L_WARN, TEXT("Process id 0x%x Exist SKIP!!"), Process->GetProcessId());

			//__debugbreak();
		}
	}
}

VOID 
CProcMgr::Remove(
	_In_ const CRefPtr<CLogEvent> pEvent
)
{
	CRefPtr<CProcess> ProcessFind = RefProcessBySeq(pEvent->GetProcSeq());
	if (!ProcessFind.IsNull()) {
		ProcessFind->MarkExit(TRUE);
		ProcessFind->SetExitEvent(pEvent);
	}else{
		LogMessage(L_INFO, TEXT("Remove process seq 0x%x is not exist in list"), pEvent->GetProcSeq());
	}
}

VOID CProcMgr::Dump()
{
#ifdef _DEBUG
	PROCESSLISTMAP::iterator it;
	int nCount;
	LogMessage(L_INFO, TEXT("===============dump process============="));
	for (it = m_ProcessList.begin(), nCount = 1; it != m_ProcessList.end(); it++, nCount++)
	{
		ULONG Seq = it->first;
		CRefPtr<CProcess> Process = it->second;
		LogMessage(L_INFO, TEXT("%d: seq=0x%x \t processId=0x%x"), nCount, Seq, Process->GetProcessId());
	}

	LogMessage(L_INFO, TEXT("======================================="));
#endif
}

VOID CProcMgr::Clear()
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
	m_ProcessList.clear();
}
