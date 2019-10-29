#pragma once

#include <vector>
#include <shared_mutex>

#define DATAVIEW()  Singleton<CDataView>::getInstance()

class CDataView
{
public:
	CDataView();
	~CDataView();

public:

	void SetSelectIndex(size_t Index);
	size_t GetSelectIndex();
	CRefPtr<COptView> GetSelectView();
	CRefPtr<COptView> GetView(size_t Index);
	size_t GetShowViewCounts();
	void ClearShowViews();
	void Push(CRefPtr<COptView> pOpt);

private:

	size_t m_SelectIndex = 0;

	//
	// 这里保存了所有的消息
	//

	std::vector<CRefPtr<COptView>> m_OptViews;

	//
	// 这里只保存需要显示的消息
	//

	std::vector<CRefPtr<COptView>> m_ShowViews;
	std::shared_mutex m_Viewlock;
};