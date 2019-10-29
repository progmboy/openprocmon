#pragma once

class COperator : public CRefBase
{
public:
	COperator() {};
	virtual ~COperator() {};

public:

	CBuffer& getPreLog();
	CBuffer& getPostLog();

	VOID setPreLog(const PLOG_ENTRY pEntry);
	VOID setPostLog(const PLOG_ENTRY pEntry);

	USHORT GetNotifyType();
	USHORT GetMoniterType();
	DWORD GetProcSeq();
	DWORD GetSeq();

	VOID SetPath(const CString& strPath)
	{
		m_strPath = strPath;
	}

	VOID SetDetail(const CString& strDetail)
	{
		m_strDetail = strDetail;
	}

	const CString& GetPath()
	{
		return m_strPath;
	}

	const CString& GetDetail()
	{
		return m_strDetail;
	}

private:

	CBuffer m_Prelog;
	CBuffer m_Postlog;

	CString m_strPath;
	CString m_strDetail;
};


