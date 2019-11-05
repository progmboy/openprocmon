#pragma once

#include "dataview.h"

#define WM_SYMBOL_PARSE WM_USER+250


class CModuleInfo : public CRefBase
{
public:
	CModuleInfo() {};
	virtual ~CModuleInfo() {};

	BOOL Init(CModule& Module);

	BOOL Init(IN HANDLE hProcess, IN HMODULE hModule);

	BOOL Init(IN const CString& strPath, IN PVOID pImageBase, IN ULONG Size);

	CString getName()
	{
		return PathFindFileName(m_strPath);
	}

	const CString& getPath()
	{
		return m_strPath;
	}
	LPVOID getBaseAddress()
	{
		return m_pBase;
	}

	ULONG getSize()
	{
		return m_Size;
	}

	BOOL IsAddressIn(LPVOID lpAddress)
	{
		if ((ULONG_PTR)lpAddress >= (ULONG_PTR)m_pBase &&
			(ULONG_PTR)(lpAddress) < ((ULONG_PTR)m_pBase + m_Size)) {
			return TRUE;
		}
		return FALSE;
	}

private:

	/** specific the module image path*/
	CString m_strPath;

	/** specific the module base address*/
	LPVOID m_pBase = NULL;

	/** specific the module image size*/
	ULONG m_Size = 0;
};

class CProcessInfo : public CRefBase
{
public:
	CProcessInfo()
	{

	}
	~CProcessInfo()
	{
	
	}

	BOOL LookupSymbolByAddress(
		IN LPVOID lpAddress,
		OUT CString& strSymbol
	);

	BOOL ListKernelModule();
	BOOL ListModule(DWORD dwProcessId);
	CRefPtr<CModuleInfo> LookupModuleByAddress(IN LPVOID lpAddress);
	BOOL ListModuleFromLog(std::vector<CModule>& modList);

private:
	std::vector<CRefPtr<CModuleInfo>> m_ModuleList;
};

class CResolveSymbolThread : public CThread
{
public:
	virtual void Run();
	void SetProcInf(CRefPtr<CProcessInfo> pProcInfo);
	void SetFrameStack(std::vector<PVOID>& FrameStack);

private:
	CRefPtr<CProcessInfo> m_ProcInfo;
	std::vector<PVOID> m_FrameStack;
};

class CPropStackDlg : public CDialogImpl<CPropStackDlg>, public CDialogResize<CPropStackDlg>
{
public:
	enum {
		IDD = PROP_STACKTRACE
	};

	BEGIN_MSG_MAP(CPropStackDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		MESSAGE_HANDLER(WM_DESTROY, OnDestroy)
		MESSAGE_HANDLER(WM_SYMBOL_PARSE, OnSymbolParse)
		CHAIN_MSG_MAP(CDialogResize<CPropStackDlg>)
	END_MSG_MAP()

	BEGIN_DLGRESIZE_MAP(CPropStackDlg)
		DLGRESIZE_CONTROL(IDC_PROP_STACKLIST, DLSZ_SIZE_X | DLSZ_SIZE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_PROPS, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SAVE, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SEARCH, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SOURCE, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_STATIC_STAUS, DLSZ_MOVE_Y | DLSZ_SIZE_X)
	END_DLGRESIZE_MAP()

	BOOL InitSymbol();
	void CleanSymbols();
	LRESULT OnSymbolParse(UINT /*uMsg*/, WPARAM wParam, LPARAM lParam, BOOL& /*bHandled*/);
	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/);
	LRESULT OnDestroy(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/);

	CString CopyAll();

private:
	CRefPtr<CProcessInfo> m_ProcInfo;
	CResolveSymbolThread m_ResoveSymbolThread;
	CListViewCtrl m_ListCtrl;
	CStatic m_StatusCtl;
	
};
