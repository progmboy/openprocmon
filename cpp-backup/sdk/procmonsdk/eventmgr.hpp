#pragma once

#include "event.hpp"
#include "eventview.hpp"

#define EVENTMGR() Singleton<CEventMgr>::getInstance()

class IProcessor : public CRefBase
{
public:
	virtual BOOL Process(const CRefPtr<CLogEvent> pEvent) = 0;
	virtual BOOL IsType(ULONG MonitorType) = 0;
};

class IEventCallback : public CRefBase
{
public:
	virtual BOOL DoEvent(const CRefPtr<CEventView> pEventView) = 0;
};

typedef std::map<ULONG, CRefPtr<CLogEvent>> EVENT_MAP;
typedef std::vector<CRefPtr<IProcessor>> PROCESSOR_LIST;


class CEventMgr
{
public:
	CEventMgr();
	virtual ~CEventMgr() {};

public:


	//
	// Call from recv msg thread
	//

	BOOL ProcessMsgBlocks(
		_In_ PLOG_ENTRY pEntries,
		_In_ ULONG Length);

	BOOL ProcessEntry(
		_In_ const PLOG_ENTRY pEntry);

	//
	// Call from process thread
	//

	BOOL Process();

	//
	// Call from global
	//

	VOID RegisterProcessor(CRefPtr<IProcessor> Processor);

	//
	// 
	//

	VOID RegisterCallback(CRefPtr<IEventCallback> pCallback);

	VOID Clear();


private:
	VOID InsertOperator(ULONG Seq, CRefPtr<CLogEvent> pEvent);
	CRefPtr<CLogEvent> RefEvent(ULONG Seq);
	VOID RemoveFromList(ULONG Seq);

	VOID PushEvent(CRefPtr<CLogEvent> pEvent);
	CRefPtr<CLogEvent> PopEvent();

private:
	EVENT_MAP m_EventMap;
	PROCESSOR_LIST m_Processors;
	std::vector<CRefPtr<IEventCallback>> m_callBackList;

	std::condition_variable m_convar;
	std::mutex m_lock;
	std::queue<CRefPtr<CLogEvent>> m_msgQueue;

	DWORD m_PushCount = 0;
	DWORD m_PopCount = 0;
};