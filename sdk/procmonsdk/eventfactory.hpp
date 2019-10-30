#pragma once

#include "event.hpp"


class CEventFactory
{
public:
	static CRefPtr<CLogEvent> CreateInstance(int EventClass);
};