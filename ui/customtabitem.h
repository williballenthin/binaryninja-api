#pragma once
#include <QtWidgets/QWidget>
#include <QtCore/QString>
#include "uitypes.h"

// Left column items
constexpr char TopLeftWidget[] = "TopLeftWidget";
constexpr char RecentFileList[] = "RecentFileList";
constexpr char OpenButtons[] = "OpenButtons";
constexpr char ReleaseNotes[] = "ReleaseNotes";

// Right column items
constexpr char TopRightWidget[] = "TopRightWidget";
constexpr char News[] = "News";

class CustomTabItem
{
public:
	typedef std::pair<QString, std::function<QWidget*(QWidget*)>> ItemNameAndCallback;
	static BINARYNINJAUIAPI void RegisterCustomTabItemAfter(const ItemNameAndCallback& newTabItem, const QString& name);
	static BINARYNINJAUIAPI std::list<ItemNameAndCallback> GetCustomTabItemsAfter(const QString& name);
private:
	static std::map<QString, std::list<ItemNameAndCallback>> m_newTabItems;
	static std::mutex m_mutex;
};
