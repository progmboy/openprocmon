#pragma once

#include "event.hpp"
#include "eventmgr.hpp"

class CProcOpt : public IProcessor
{
public:
	virtual BOOL Process(const CRefPtr<CLogEvent> pEvent);
	virtual BOOL IsType(ULONG MonitorType);
};

class CProcEvent : public CLogEvent
{
public:
	virtual CString GetPath();
	virtual CString GetDetail();
};
