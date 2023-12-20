#pragma once

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
	CFilter(MAP_SOURCE_TYPE Src, FILTER_CMP_TYPE Cmp, FILTER_RESULT_TYPE Ret, const CString& strFilter);
	~CFilter();

	BOOL BeginWith(const CString& strSrc, const CString& strDst);
	BOOL EndWith(const CString& strSrc, const CString& strDst);
	BOOL Is(const CString& strSrc, const CString& strDst);
	BOOL IsNot(const CString& strSrc, const CString& strDst);
	BOOL Lessthan(const CString& strSrc, const CString& strDst);
	BOOL Morethan(const CString& strSrc, const CString& strDst);
	BOOL Contains(const CString& strSrc, const CString& strDst);
	BOOL NotContains(const CString& strSrc, const CString& strDst);
	BOOL Match(const CRefPtr<CEventView> pOptView);

	//BOOL FilterTest(const CString& strSrc);


	MAP_SOURCE_TYPE GetSourceType()
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
	MAP_SOURCE_TYPE m_SrcType;
	FILTER_CMP_TYPE m_CmpType;
	FILTER_RESULT_TYPE m_ResultType;
	CString m_strFilter;
};