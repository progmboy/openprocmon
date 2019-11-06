#pragma once

#include "singleton.hpp"
#include "process.hpp"
#include <map>
#include <shared_mutex>

#define PROCMGR()  Singleton<CProcMgr>::getInstance()

typedef std::map<ULONG, CRefPtr<CProcess>> PROCESSLISTMAP;
typedef std::pair<ULONG, CRefPtr<CProcess>> PROCESSLISTMAPPAIR;

class CProcMgr
{
public:
	CProcMgr();
	~CProcMgr();

public:

	CRefPtr<CProcess> RefProcessBySeq(
		_In_ ULONG Seq
	);

	CRefPtr<CProcess> RefProcessByProcessId(
		_In_ ULONG ProcessId
	);

	VOID Insert(
		_In_ CRefPtr<CProcess> Process
	);

	VOID InsertModule(
		_In_ ULONG ProcSeq,
		_In_ PLOG_LOADIMAGE_INFO pInfo
	);

	VOID Replace(
		_In_ ULONG Seq, 
		_In_ CRefPtr<CProcess> Process
	);

	VOID Remove(
		_In_ const CRefPtr<CLogEvent> pEvent
	);

	VOID Dump();

	VOID Clear();

private:

	
	//
	// 这里只有一个线程去处理数据,所以我们没必要加锁
	//
	
	std::shared_mutex m_lock;
	PROCESSLISTMAP m_ProcessList;

};
