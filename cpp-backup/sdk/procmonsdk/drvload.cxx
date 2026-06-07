
#include "pch.hpp"

#include "drvload.hpp"
#include "logger.hpp"
#include "utils.hpp"
#include <winternl.h>

#pragma comment(lib, "ntdll.lib")

#ifndef NT_SUCCESS
#define NT_SUCCESS(Status) (((NTSTATUS)(Status)) >= 0)
#endif

EXTERN_C
NTSYSAPI
NTSTATUS
NtLoadDriver(
	PUNICODE_STRING DriverServiceName
);

EXTERN_C
NTSYSAPI
NTSTATUS
NtUnloadDriver(
	PUNICODE_STRING DriverServiceName
);

#define REGISTRY_PATH_PREFIX		TEXT("System\\CurrentControlSet\\Services\\")
#define SERVICE_IMAGE_PATH_PREFIX	TEXT("\\??\\")
#define DRIVER_SERVICE_NAME_PREFIX	L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\"

CDrvLoader::CDrvLoader()
{

}

CDrvLoader::~CDrvLoader()
{

}

BOOL 
CDrvLoader::Init(
	IN const CString& strDriverName, 
	IN const CString& strDriverPath)
{

	CPath DriverPath(strDriverPath);
	if (DriverPath.IsRelative()){
		TCHAR szFullName[MAX_PATH] = { 0 };
		GetModuleFileName(NULL, szFullName, MAX_PATH);
		PathRemoveFileSpec(szFullName);
		PathAppend(szFullName, DriverPath);
		DriverPath = szFullName;
	}

	if(!DriverPath.FileExists()){
		LogMessage(L_WARN, TEXT("Driver file not exist"));
		return FALSE;
	}else{

		//
		// Try enable driver load privilege
		//

		if (UtilSetPriviledge(SE_LOAD_DRIVER_NAME, TRUE)){
			m_strDriverPath = (LPCTSTR)DriverPath;
			m_strDriverName = strDriverName;

			return TRUE;
		}
	}
	return FALSE;
}

#define STATUS_IMAGE_ALREADY_LOADED ((NTSTATUS)0xC000010EL)

BOOL CDrvLoader::Load()
{
	
	//
	// Delete the service first
	//

	//DeleteServiceKey();

	if (!CreateServiceKey()){
		return FALSE;
	}

	CStringW strDriverSrvName;
	UNICODE_STRING UniDrvSrvName;
	NTSTATUS Status;


	strDriverSrvName = DRIVER_SERVICE_NAME_PREFIX;
	strDriverSrvName += CT2W(m_strDriverName);

	RtlInitUnicodeString(&UniDrvSrvName, strDriverSrvName);
	Status = NtLoadDriver(&UniDrvSrvName);

	if (!NT_SUCCESS(Status) && Status != STATUS_IMAGE_ALREADY_LOADED){
		LogMessage(L_ERROR, TEXT("NtLoadDriver Failed code 0x%x"), Status);
		return FALSE;
	}

	return TRUE;
	
}

BOOL CDrvLoader::UnLoad()
{
	CStringW strDrvSrvName;
	UNICODE_STRING UniDrvSrvName;
	NTSTATUS Status;

	strDrvSrvName = DRIVER_SERVICE_NAME_PREFIX;
	strDrvSrvName += m_strDriverName;

	RtlInitUnicodeString(&UniDrvSrvName, strDrvSrvName);
	Status = NtUnloadDriver(&UniDrvSrvName);
	if (!NT_SUCCESS(Status)) {
		LogMessage(L_ERROR, TEXT("NtUnloadDriver Failed code 0x%x"), Status);
		return FALSE;
	}

	return TRUE;
}

BOOL CDrvLoader::CreateServiceInstanceKey(HKEY hKey)
{
	HKEY hKeySubIns;
	HKEY hKeyInstance;
	CString strTemp;
	DWORD Data = 3;

	RegSetValueEx(hKey, TEXT("SupportedFeatures"), 0, REG_DWORD, (const BYTE*)&Data, sizeof(DWORD));
	RegCreateKey(hKey, TEXT("Instances"), &hKeyInstance);

	strTemp = TEXT("Process Monitor 24 Instance");
	RegSetValueEx(hKeyInstance, TEXT("DefaultInstance"), 0, REG_SZ, (const BYTE*)strTemp.GetBuffer(),
		(DWORD)(((DWORD)strTemp.GetLength() + 1) * sizeof(TCHAR)));
	RegCreateKey(hKeyInstance, TEXT("Process Monitor 24 Instance"), &hKeySubIns);

	strTemp = TEXT("385200");
	RegSetValueEx(hKeySubIns, TEXT("Altitude"), 0, REG_SZ, (const BYTE*)strTemp.GetBuffer(), 
		(DWORD)(((DWORD)strTemp.GetLength() + 1) * sizeof(TCHAR)));
	Data = 0;
	RegSetValueEx(hKeySubIns, TEXT("Flags"), 0, REG_DWORD, (const BYTE*)&Data, sizeof(DWORD));

	RegCloseKey(hKeySubIns);
	RegCloseKey(hKeyInstance);

	return TRUE;
}

BOOL CDrvLoader::CreateServiceKey()
{
	CString strRegistryPath;
	HKEY hKey;
	LSTATUS dwErrorCode;
	DWORD dwDisposition;
	CString strServiceImagePath;
	DWORD dwImagPathSize;

	if (!IsReady()) {
		return FALSE;
	}

	//
	// Format registry path
	//

	strRegistryPath = REGISTRY_PATH_PREFIX;
	strRegistryPath += m_strDriverName;

	//
	// Convert dos path to nt path
	//

	strServiceImagePath = SERVICE_IMAGE_PATH_PREFIX;
	strServiceImagePath += m_strDriverPath;

	dwErrorCode = RegCreateKeyEx(HKEY_LOCAL_MACHINE,
		strRegistryPath.GetBuffer(),
		0,
		NULL,
		0,
		KEY_ALL_ACCESS,
		NULL,
		&hKey,
		&dwDisposition);
	if (ERROR_SUCCESS != dwErrorCode) {
		return FALSE;
	}

	//
	// The key exist?
	//

	if (dwDisposition != REG_CREATED_NEW_KEY) {
		LogMessage(L_WARN, TEXT("RegCreateKeyEx return REG_CREATED_NEW_KEY"));
		//RegCloseKey(hKey);
		//return TRUE;
	}

	dwImagPathSize = (DWORD)(((DWORD)strServiceImagePath.GetLength() + 1) * sizeof(TCHAR));
	dwErrorCode = RegSetValueEx(hKey,
		TEXT("ImagePath"),
		0,
		REG_EXPAND_SZ,
		(const BYTE*)strServiceImagePath.GetBuffer(),
		dwImagPathSize);
	if (ERROR_SUCCESS != dwErrorCode) {
		RegCloseKey(hKey);
		return FALSE;
	}

	//
	// Set type
	//

	DWORD dwServiceType = 1;
	dwErrorCode = RegSetValueEx(hKey,
		TEXT("Type"),
		0,
		REG_DWORD,
		(const BYTE*)&dwServiceType,
		sizeof(dwServiceType));
	if (dwErrorCode) {
		RegCloseKey(hKey);
		return dwErrorCode;
	}

	//
	// Set error control
	//

	DWORD dwServiceErrorControl = 1;
	dwErrorCode = RegSetValueEx(hKey,
		TEXT("ErrorControl"),
		0,
		REG_DWORD,
		(const BYTE*)&dwServiceErrorControl,
		sizeof(DWORD));
	if (dwErrorCode) {
		RegCloseKey(hKey);
		return dwErrorCode;
	}

	DWORD dwServiceStart = 3;
	dwErrorCode = RegSetValueEx(hKey,
		TEXT("Start"),
		0,
		REG_DWORD,
		(const BYTE*)&dwServiceStart,
		sizeof(dwServiceStart));

	CreateServiceInstanceKey(hKey);

	//
	// Finish cleanup
	//

	RegCloseKey(hKey);
	return TRUE;
}

VOID CDrvLoader::DeleteServiceKey()
{
	CString strRegistryPath;
	
	if (!IsReady()){
		return;
	}

	//
	// Format service registry path
	//

	strRegistryPath = REGISTRY_PATH_PREFIX;
	strRegistryPath += m_strDriverName;

	SHDeleteKey(HKEY_LOCAL_MACHINE, strRegistryPath);
}

BOOL CDrvLoader::IsReady()
{
	if (!m_strDriverName ||
		!m_strDriverPath) {
		return FALSE;
	}
	return TRUE;
}
