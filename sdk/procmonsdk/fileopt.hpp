#pragma once

class CFileOpt : public IProcessor
{
public:
	virtual BOOL Process(const CRefPtr<COperator> Operator);
	virtual BOOL IsType(ULONG MonitorType);
	virtual BOOL Parse(const CRefPtr<COperator> Operator);
private:

};