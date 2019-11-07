
#include "stdafx.h"
#include "filtermgr.h"

BOOL CFilterMgr::Filter(CRefPtr<CEventView> pView)
{
	std::shared_lock<std::shared_mutex> lock(m_lock);
	BOOL bFiler = FALSE;
	for (auto it = m_FilterList.begin(); it != m_FilterList.end(); it++)
	{
		if((*it)->Filter(pView)){
			bFiler = TRUE;
			break;
		}
	}

	return bFiler;
}

size_t CFilterMgr::GetCounts()
{
	return m_FilterList.size();
}

void CFilterMgr::AddFilter(
	MAP_SOURCE_TYPE SrcType, 
	FILTER_CMP_TYPE CmpType, 
	FILTER_RESULT_TYPE RetType, 
	const CString& strDst
)
{
	AddFilter(new CFilter(SrcType, CmpType, RetType, strDst));
}

void CFilterMgr::AddFilter(CRefPtr<CFilter> pFilter)
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
	m_FilterList.insert(m_FilterList.begin(), pFilter);
}

void CFilterMgr::AddFilterAtEnd(CRefPtr<CFilter> pFilter)
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
	m_FilterList.insert(m_FilterList.end(), pFilter);
}

void CFilterMgr::RemovFilter(
	MAP_SOURCE_TYPE SrcType,
	FILTER_CMP_TYPE CmpType,
	FILTER_RESULT_TYPE RetType,
	const CString& strDst
)
{
	for (auto it = m_FilterList.begin(); it != m_FilterList.end();)
	{
		if ((*it)->GetSourceType() == SrcType &&
			(*it)->GetCmpType() == CmpType && 
			(*it)->GetRetType() == RetType &&
			(*it)->GetFilter() == strDst){
			it = m_FilterList.erase(it);
		}else{
			it++;
		}
	}
}

void CFilterMgr::RemovFilter(CRefPtr<CFilter> pFilter)
{
	RemovFilter(pFilter->GetSourceType(), pFilter->GetCmpType(),
		pFilter->GetRetType(), pFilter->GetFilter());
}
