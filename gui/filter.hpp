#pragma once

typedef enum _FILTER_SOURCE_TYPE
{
	emArchiteture,
	emAuthId,
	emCategory,
	emCommandLine,
	emCompany,
	emCompletionTime,
	emDataTime,
	emDescription,
	emDetail,
	emDuration,
	emEventClass,
	emImagePath,
	emIntegrity,
	emOperation,
	emParentPid,
	emPath,
	emPID,
	emProcessName,
	emRelativeTime,
	emResult,
	emSequence,
	emSession,
	emTID,
	emTimeOfDay,
	emUser,
	emVersion,
	emVirtualize
}FILTER_SOURCE_TYPE;

typedef enum _FILTER_CMP_TYPE
{
	emCMPIs,
	emCMPIsNot,
	emCMPLessThan,
	emCMPMoreThan,
	emCMPBeginWith,
	emCMPEndWith,
	emCMPContains,
	emCMPExcludes
}FILTER_CMP_TYPE;

typedef enum _FILTER_RESULT_TYPE
{
	emRETInclude,
	emRETExclude
}FILTER_RESULT_TYPE;

class CFilter : public CRefBase
{
public:
	CFilter(FILTER_SOURCE_TYPE Src, FILTER_CMP_TYPE Cmp, FILTER_RESULT_TYPE Ret, const CString& strFilter);
	~CFilter();

	CString GetSrc(FILTER_SOURCE_TYPE SrcType, const CRefPtr<CEventView> pOptView);
	BOOL BeginWith(const CString& strSrc, const CString& strDst);
	BOOL EndWith(const CString& strSrc, const CString& strDst);
	BOOL Is(const CString& strSrc, const CString& strDst);
	BOOL IsNot(const CString& strSrc, const CString& strDst);
	BOOL Lessthan(const CString& strSrc, const CString& strDst);
	BOOL Morethan(const CString& strSrc, const CString& strDst);
	BOOL Contains(const CString& strSrc, const CString& strDst);
	BOOL NotContains(const CString& strSrc, const CString& strDst);
	BOOL Filter(const CRefPtr<CEventView> pOptView);

	BOOL FilterTest(const CString& strSrc);


	FILTER_SOURCE_TYPE GetSourceType()
	{
		return m_SrcType;
	}

	FILTER_CMP_TYPE GetCmpType()
	{
		return m_CmpType;
	}

	FILTER_RESULT_TYPE GetRetType()
	{
		return m_ResultType;
	}

	const CString& GetFilter()
	{
		return m_strFilter;
	}

private:
	FILTER_SOURCE_TYPE m_SrcType;
	FILTER_CMP_TYPE m_CmpType;
	FILTER_RESULT_TYPE m_ResultType;
	CString m_strFilter;
};