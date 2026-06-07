
#include "stdafx.h"
#include "filtermgr.h"

BOOL CFilterMgr::Filter(CRefPtr<CEventView> pView)
{
	std::shared_lock<std::shared_mutex> lock(m_lock);
	
	BOOL bFilter = FALSE;
	for (auto filter : m_FilterList) {
		if (filter->Filter(pView)) {
			bFilter = TRUE;
			break;
		}
	}

	return bFilter;
}

size_t CFilterMgr::GetCounts()
{
	return m_FilterList.size();
}

void CFilterMgr::AddFilter(
	MAP_SOURCE_TYPE SrcType, 
	FILTER_CMP_TYPE CmpType, 
	FILTER_RESULT_TYPE RetType, 
	const CString& strDst,
	BOOL Enable
)
{
	AddFilter(new CFilter(SrcType, CmpType, RetType, strDst, Enable));
}

void CFilterMgr::AddFilter(CRefPtr<CFilter> pFilter)
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
	m_FilterList.insert(m_FilterList.begin(), pFilter);
	//Sort();
}

void CFilterMgr::RemovFilter(
	MAP_SOURCE_TYPE SrcType,
	FILTER_CMP_TYPE CmpType,
	FILTER_RESULT_TYPE RetType,
	const CString& strDst
)
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
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
	std::unique_lock<std::shared_mutex> lock(m_lock);
	std::sort(m_FilterList.begin(), m_FilterList.end(), CFilterCompare());
}

const std::vector<CRefPtr<CFilter>>& CFilterMgr::GetFilterList()
{
	return m_FilterList;
}

void CFilterMgr::SetEnable(ULONG Index, BOOL Enable)
{
	if (Index < m_FilterList.size()) {
		m_FilterList[Index]->SetEnable(Enable);
	}
}

BOOL CFilterMgr::IsEnable(ULONG Index)
{
	if (Index < m_FilterList.size()) {
		return m_FilterList[Index]->IsEnable();
	}

	return FALSE;
}

void CFilterMgr::RemoveAll()
{
	std::unique_lock<std::shared_mutex> lock(m_lock);
	m_FilterList.clear();
}

