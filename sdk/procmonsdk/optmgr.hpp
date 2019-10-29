#pragma once

#include <map>
#include <vector>
#include <mutex>
#include <condition_variable>
#include <queue>

#include "operator.hpp"
#include "optview.hpp"

#define OPERATORMGR() Singleton<COperatorMgr>::getInstance()

class IProcessor : public CRefBase
{
public:
	virtual BOOL Process(const CRefPtr<COperator> Operator) = 0;
	virtual BOOL IsType(ULONG MonitorType) = 0;
	virtual BOOL Parse(const CRefPtr<COperator> Operator) = 0;
};

class IEventCallback : public CRefBase
{
public:
	virtual BOOL DoEvent(const CRefPtr<COptView> pEventView) = 0;
};

typedef std::map<ULONG, CRefPtr<COperator>> OPERATOR_MAP;
typedef std::vector<CRefPtr<IProcessor>> PROCESSOR_LIST;


class COperatorMgr
{
public:
	COperatorMgr();
	virtual ~COperatorMgr() {};

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
	CRefPtr<COperator> RefOperator(ULONG Seq);
	VOID RemoveFromList(ULONG Seq);

	VOID PushOpt(CRefPtr<COperator> opt);
	CRefPtr<COperator> PopOpt();

private:
	OPERATOR_MAP m_OperatorMap;
	PROCESSOR_LIST m_Processors;
	std::vector<CRefPtr<IEventCallback>> m_callBackList;

	std::condition_variable m_convar;
	std::mutex m_lock;
	std::queue<CRefPtr<COperator>> m_msgQueue;

	DWORD m_PushCount = 0;
	DWORD m_PopCount = 0;
};