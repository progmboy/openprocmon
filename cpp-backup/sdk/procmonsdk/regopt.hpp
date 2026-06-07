#pragma once

class CRegEvent : public CLogEvent
{
public:
	virtual CString GetPath();

	virtual CString GetDetail()
	{
		return TEXT("TODO");
	}
};