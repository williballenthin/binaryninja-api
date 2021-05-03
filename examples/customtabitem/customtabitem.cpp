#include <QtWidgets/QLabel>
#include <QtWidgets/QListView>
#include <QtWidgets/QSizePolicy>
#include <QtCore/QStringListModel>
#include "uitypes.h"
#include "customtabitem.h"

extern "C"
{
	BN_DECLARE_UI_ABI_VERSION
	BINARYNINJAPLUGIN bool UIPluginInit()
	{
		auto createItem = [](QWidget* parent) -> QWidget*
		{
			auto list = new QListView(parent);
			list->resize(parent->width(), 240);
			list->setModel(new QStringListModel({"nevins.bin", "hamlin.bin"}, parent));
			list->setSizePolicy(QSizePolicy(QSizePolicy::Fixed, QSizePolicy::Fixed));
			return list;
		};
		CustomTabItem::RegisterCustomTabItemAfter({"MyCustomTabItem", createItem}, TopRightWidget);
		return true;
	}
}
