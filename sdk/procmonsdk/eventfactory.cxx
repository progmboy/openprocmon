
#include "pch.hpp"
#include "event.hpp"
#include "procopt.hpp"
#include "fileopt.hpp"
#include "regopt.hpp"
#include "eventfactory.hpp"

CRefPtr<CLogEvent> CEventFactory::CreateInstance(int EventClass)
{
	switch (EventClass)
	{
	case MONITOR_TYPE_PROCESS:
		return new CProcEvent;
	case MONITOR_TYPE_FILE:
		return new CFileEvent;
	case MONITOR_TYPE_REG:
		return new CRegEvent;
	default:
		return NULL;
	}
}

