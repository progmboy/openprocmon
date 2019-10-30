

#include "stdafx.h"
#include "filter.hpp"
#include "status.h"

CFilter::CFilter(
	FILTER_SOURCE_TYPE Src, 
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


CString
CFilter::GetSrc(
	FILTER_SOURCE_TYPE SrcType,
	const CRefPtr<COptView> pOptView)
{
	CString strSrc;
	switch (SrcType)
	{
	case emArchiteture:
		strSrc = pOptView->IsWow64() ? TEXT("32-bit") : TEXT("64-bit");
		break;
	case emAuthId:
		LUID AuthId = pOptView->GetAuthId();
		strSrc.Format(TEXT("%08x:%08x"), AuthId.HighPart, AuthId.LowPart);
		break;
	case emCategory:
		break;
	case emCommandLine:
		strSrc = pOptView->GetCommandLine();
		break;
	case emCompany:
		strSrc = pOptView->GetCompanyName();
		break;
	case emCompletionTime:
		break;
	case emDataTime:
		break;
	case emDescription:
		strSrc = pOptView->GetDisplayName();
		break;
	case emDetail:
		strSrc = pOptView->GetDetail();
		break;
	case emDuration:
		
		//
		// TODO
		//
		
		break;
	case emEventClass:
		strSrc = GetClassStringMap(pOptView->GetEventClass());
		break;
	case emImagePath:
		strSrc = pOptView->GetImagePath();
		break;
	case emIntegrity:
		
		//
		// TODO
		//
		
		break;
	case emOperation:
		strSrc = GetOperatorStringMap(pOptView->GetPreEventEntry());
		break;
	case emParentPid:
		strSrc.Format(TEXT("%d"), pOptView->GetParentProcessId());
		break;
	case emPath:
		strSrc = pOptView->GetPath();
		break;
	case emPID:
		strSrc.Format(TEXT("%d"), pOptView->GetProcessId());
		break;
	case emProcessName:
		strSrc = pOptView->GetProcessName();
		break;
	case emRelativeTime:
		
		//
		// TODO
		//
		
		break;
	case emResult:
		strSrc = StatusGetDesc(pOptView->GetResult());
		break;
	case emSequence:
		strSrc.Format(TEXT("%lu"), pOptView->GetSeqNumber());
		break;
	case emSession:
		strSrc.Format(TEXT("%u"), pOptView->GetSessionId());
		break;
	case emTID:
		strSrc.Format(TEXT("%d"), pOptView->GetThreadId());
		break;
	case emTimeOfDay:
		
		//
		// TODO
		//
		
		break;
	case emUser:
		
		//
		// TODO
		//
		
		break;
	case emVersion:
		strSrc = pOptView->GetVersion();
		break;
	case emVirtualize:
		strSrc = pOptView->IsVirtualize() ? TEXT("True") : TEXT("False");
	default:
		break;
	}

	return strSrc;
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
CFilter::Filter(
	const CRefPtr<COptView> pOptView
)
{
	BOOL bFilter = FALSE;
	CString strSrc;
	strSrc = GetSrc(m_SrcType, pOptView);
	
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

	bFilter = m_ResultType == emRETInclude ? !bCompare : bCompare;

	return bFilter;
}

BOOL
CFilter::FilterTest(
	const CString& strSrc
)
{
	BOOL bFilter;
	BOOL bCompare = FALSE;
	if (m_CmpType <= emCMPExcludes && m_CmpType >= emCMPIs) {
		bCompare = (this->*gFunctionTable[m_CmpType])(strSrc, m_strFilter);
	}

	bFilter = m_ResultType == emRETInclude ? !bCompare : bCompare;

	return bFilter;
}