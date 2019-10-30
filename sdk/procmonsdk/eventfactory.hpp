#pragma once

class CEventFactory
{
public:
	static CRefPtr<CLogEvent> CreateInstance(int EventClass);
};