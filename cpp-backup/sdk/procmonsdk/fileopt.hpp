#pragma once


class CFileEvent : public CLogEvent
{
public:
	virtual CString GetPath();
	virtual CString GetDetail();
};