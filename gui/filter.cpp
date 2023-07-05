

#include "stdafx.h"
#include "filter.hpp"

CFilter::CFilter(
	MAP_SOURCE_TYPE Src, 
	FILTER_CMP_TYPE Cmp, 
	FILTER_RESULT_TYPE Ret, 
	const CString& strFilter
):m_SrcType(Src), m_CmpType(Cmp), m_ResultType(Ret),
m_strFilter(strFilter)
{
	
}

CFilter::~CFilter()
{

}

typedef BOOL (CFilter::* CMPFUNCTION)(const CString& strSrc, const CString& strDst);

BOOL
CFilter::BeginWith(const CString& strSrc, const CString& strDst)
{
	if (strSrc.GetLength() < strDst.GetLength()){
		return FALSE;
	}

	return strSrc.Left(strDst.GetLength()).CompareNoCase(strDst) == 0;
}

BOOL
CFilter::EndWith(const CString& strSrc, const CString& strDst)
{
	if (strSrc.GetLength() < strDst.GetLength()) {
		return FALSE;
	}

	return strSrc.Right(strDst.GetLength()).CompareNoCase(strDst) == 0;
}

BOOL
CFilter::Is(const CString& strSrc, const CString& strDst)
{
	if (strSrc.GetLength() != strDst.GetLength()) {
		return FALSE;
	}

	return strSrc.CompareNoCase(strDst) == 0;
}

BOOL
CFilter::IsNot(const CString& strSrc, const CString& strDst)
{
	return !Is(strSrc, strDst);
}

BOOL
CFilter::Lessthan(const CString& strSrc, const CString& strDst)
{
	return strSrc.CompareNoCase(strDst) < 0;
}

BOOL
CFilter::Morethan(const CString& strSrc, const CString& strDst)
{
	return strSrc.CompareNoCase(strDst) > 0;
}

BOOL
CFilter::Contains(const CString& strSrc, const CString& strDst)
{
	CString strSrcTmp = strSrc;
	CString strDstTmp = strDst;

	return strSrcTmp.MakeUpper().Find(strDstTmp.MakeUpper()) != -1;
}

BOOL
CFilter::NotContains(const CString& strSrc, const CString& strDst)
{
	return !Contains(strSrc, strDst);
}

typedef struct _CMP_TABLE
{
	CMPFUNCTION CmpFunction;
}CMP_TABLE;

CMPFUNCTION gFunctionTable[] =
{
	& CFilter::Is,
	& CFilter::IsNot,
	& CFilter::Lessthan,
	& CFilter::Morethan,
	& CFilter::BeginWith,
	& CFilter::EndWith,
	& CFilter::Contains,
	& CFilter::NotContains,
};

BOOL 
CFilter::Match(
	const CRefPtr<CEventView> pOptView
)
{
	CString strSrc;
	strSrc = pOptView->GetOperationStrResult(m_SrcType);
	
	//
	// Do not filter if is empty
	//
	
	if (strSrc.IsEmpty()){
		return FALSE;
	}
	
	BOOL bCompare = FALSE;
	if (m_CmpType <= emCMPExcludes && m_CmpType >= emCMPIs){
		bCompare = (this->*gFunctionTable[m_CmpType])(strSrc, m_strFilter);
	}

	return bCompare;
}


