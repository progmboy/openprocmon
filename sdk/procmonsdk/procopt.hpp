#pragma once

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
	virtual CString GetPath()
	{
		return TEXT("TODO");
	}

	virtual CString GetDetail()
	{
		return TEXT("TODO");
	}
};
