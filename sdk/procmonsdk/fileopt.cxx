
#include "pch.hpp"
#include "procmgr.hpp"
#include "fileopt.hpp"
#include "eventview.hpp"
#include "strmaps.hpp"


CString CFileEvent::GetPath()
{
	PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(getPreLog().GetBuffer());
	PLOG_FILE_OPT pFileOpt = TO_EVENT_DATA(PLOG_FILE_OPT, pEntry);

	CString strFileName;

	if (pFileOpt->NameLength){
		CString strFileNameTmp;
		strFileNameTmp.Append(pFileOpt->Name, pFileOpt->NameLength);
		UtilConvertNtInternalPathToDosPath(strFileNameTmp, strFileName);
	}
	

	return strFileName;
}

CString CFileEvent::GetDetail()
{
	PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(getPreLog().GetBuffer());
	PLOG_ENTRY pPostEntry = reinterpret_cast<PLOG_ENTRY>(getPostLog().GetBuffer());

	PLOG_FILE_OPT pFileOpt = TO_EVENT_DATA(PLOG_FILE_OPT, pEntry);
	UCHAR MajorFunction = pEntry->NotifyType - 20;
	CString strDetail;

	switch (MajorFunction)
	{
	case IRP_MJ_CREATE:
	{
		CString strTemp;

		PLOG_FILE_CREATE pCreateInfo = reinterpret_cast<PLOG_FILE_CREATE>(pFileOpt->Name + pFileOpt->NameLength);

		strTemp.Format(TEXT("DesiredAccess:(0x%x) %s"), 
			pCreateInfo->DesiredAccess,
			(LPCTSTR)StrMapFileAccessMask(pCreateInfo->DesiredAccess));

		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		strTemp.Format(TEXT("Io Status: %s"), StrMapNtStatus(pPostEntry->Status));

		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		strTemp.Format(TEXT("AllocationSize: 0x%llx"), 
			pFileOpt->FltParameter.Create.AllocationSize.QuadPart);
		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		strTemp.Format(TEXT("FileAttributes: (0x%x) %s"), 
			pFileOpt->FltParameter.Create.FileAttributes,
			(LPCTSTR)StrMapFileAttributes(pFileOpt->FltParameter.Create.FileAttributes));
		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		strTemp.Format(TEXT("SharedAccess: (0x%x) %s"), 
			pFileOpt->FltParameter.Create.ShareAccess,
			StrMapFileShareAccess(pFileOpt->FltParameter.Create.ShareAccess).GetBuffer());
		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		DWORD CreateOptions = pFileOpt->FltParameter.Create.Options & 0x00FFFFFF;
		DWORD CreateDisposition = pFileOpt->FltParameter.Create.Options >> 24;

		strTemp.Format(TEXT("CreateOptions: (0x%x) %s"), 
			CreateOptions, 
			(LPCTSTR)StrMapFileCreateOptions(CreateOptions));
		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		strTemp.Format(TEXT("CreateDisposition:(0x%x) %s"), 
			CreateDisposition,
			StrMapFileCreateDisposition(CreateDisposition));
		strDetail += strTemp;
		strDetail += TEXT("\r\n");

		ULONG_PTR* pInformation = TO_EVENT_DATA(ULONG_PTR*, pPostEntry);

		strTemp.Format(TEXT("RetDisposition:(0x%x) %s"),
			(DWORD)*pInformation,
			StrMapFileRetDisposition((DWORD)*pInformation));
		strDetail += strTemp;
		strDetail += TEXT("\r\n");
		
	}
		break;

	case IRP_MJ_SET_SECURITY:
	{
		CString strTemp;
		strTemp.Format(TEXT("SecurityInformation: %s"), 
			StrMapSecurityInformation(pFileOpt->FltParameter.SetSecurity.SecurityInformation).GetBuffer());
		strDetail += strTemp;
		strDetail += TEXT("\r\n");
	}
	break;
	default:
		strDetail = TEXT("TODO");
		break;
	}

	return strDetail;
}
