#pragma once

#include "filter.hpp"
#include <vector>
#include <shared_mutex>

#define FILETERMGR() Singleton<CFilterMgr>::getInstance()

class CFilterMgr
{
public:
	CFilterMgr(){}
	~CFilterMgr() {}

public:

	BOOL Filter(CRefPtr<CEventView> pView);
	void AddFilter(CRefPtr<CFilter> pFilter);
	void AddFilter(MAP_SOURCE_TYPE SrcType, FILTER_CMP_TYPE CmpType, FILTER_RESULT_TYPE RetType, const CString& strDst);
	void AddFilterEnd(CRefPtr<CFilter> pFilter);
	void RemovFilter(MAP_SOURCE_TYPE SrcType, FILTER_CMP_TYPE CmpType, FILTER_RESULT_TYPE RetType, const CString& strDst);
private:

	std::shared_mutex m_lock;
	std::vector<CRefPtr<CFilter>> m_FilterList;
};