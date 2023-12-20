
#include "stdafx.h"
#include "filtermgr.h"

BOOL CFilterMgr::Filter(CRefPtr<CEventView> pView)
{
	std::shared_lock<std::shared_mutex> lock(m_lock);
	
	BOOL bIncludeStart = FALSE;

	//
	// Before filter the list of contain all of filters must be sort.
	// Filter list like:
	//
	// exclude filter 1
	// exclude filter 2
	// ....
	// include filter 1
	// include filter 2
	// ...
	//
	// we match exclude filter first

	for (auto filter : m_FilterList) {

		if (!bIncludeStart && filter->GetRetType() == FILTER_RESULT_TYPE::emRETInclude) {
			bIncludeStart = TRUE;
		}

		if(!bIncludeStart) {
			if (filter->Match(pView)) {
				// Here must be in exclude.
				// If filter match we need drop this event.
				return TRUE;
			}
		}else{
			if(filter->Match(pView)) {
				return FALSE;
			}
		}
	}

	if (!bIncludeStart){
		return FALSE;
	}

	return TRUE;
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
	Sort();
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

class CFilterCompare
{
public:
	bool operator() (const CRefPtr<CFilter> p1, const CRefPtr<CFilter> p2)
	{
		return p1->GetRetType() > p2->GetRetType();
	}
};

void CFilterMgr::Sort()
{
	std::sort(m_FilterList.begin(), m_FilterList.end(), CFilterCompare());
}

