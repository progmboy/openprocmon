#pragma once


template<typename T>
class Singleton
{
public:
	static T& getInstance()
	{
		static T value;
		return value;
	}

private:
	Singleton();
	~Singleton();
};