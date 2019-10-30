#pragma once

class CLogEvent : public CRefBase
{
public:
	CLogEvent() {};
	virtual ~CLogEvent() {};

public:

	CBuffer& getPreLog();
	CBuffer& getPostLog();

	VOID setPreLog(const PLOG_ENTRY pEntry);
	VOID setPostLog(const PLOG_ENTRY pEntry);

	USHORT GetNotifyType();
	USHORT GetMoniterType();
	DWORD GetProcSeq();
	DWORD GetSeq();

public:
	virtual CString GetPath();
	virtual CString GetDetail();

private:

	CBuffer m_Prelog;
	CBuffer m_Postlog;
};


