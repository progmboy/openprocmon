
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
	PROCESSLISTMAP::iterator it;
	it = m_ProcessList.find(Seq);
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

		m_ProcessList.insert(PROCESSLISTMAPPAIR(Seq, Process));
	}
	else {

		if (ProcessFind->IsMarkExit()) {

			//
			// remove from list
			//

			m_ProcessList.erase(Seq);

			//
			// Replace it with new one
			//

			m_ProcessList.insert(PROCESSLISTMAPPAIR(Seq, Process));

		}else{
			
			//
			// Here must be something wrong
			//
			
			if (!Process->GetProcessId()) {
				Process->Dump();
			}

			LogMessage(L_WARN, TEXT("Process id 0x%x Exist SKIP!!"), Process->GetProcessId());

			__debugbreak();
		}


	}
}

VOID 
CProcMgr::Remove(
	_In_ ULONG Seq
)
{
	CRefPtr<CProcess> ProcessFind = RefProcessBySeq(Seq);
	if (!ProcessFind.IsNull()) {
		//LogMessage(L_INFO, TEXT("Process id 0x%x seq 0x%x mark with exit"), ProcessFind->GetProcessId(), Seq);
		//m_ProcessList.erase(Seq);
		ProcessFind->MarkExit(TRUE);
	}else{
		LogMessage(L_INFO, TEXT("Remove process seq 0x%x is not exist in list"), Seq);
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
	m_ProcessList.clear();
}
