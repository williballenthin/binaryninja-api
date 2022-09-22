#pragma once

#include <QtWidgets/QComboBox>
#include <QtWidgets/QDialog>
#include <QtWidgets/QLabel>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtWidgets/QTabWidget>
#include <QtWidgets/QWidget>
#include "binaryninjaapi.h"
#include "viewtype.h"
#include "filecontext.h"

#include <string>
#include <tuple>
#include <vector>

class BINARYNINJAUIAPI DebugInfoImport : public QDialog
{
	Q_OBJECT

  BinaryNinja::Ref<BinaryNinja::DebugInfo> m_debugInfo;
  BinaryViewRef m_bv;

	QLabel* m_fileLabel;
	QLabel* m_objectLabel;
	QComboBox* m_objectCombo;
	QTabWidget* m_tab;
	QLabel* m_notification;
	QPushButton* m_defaultsButton;

	FileContext* m_file = nullptr;
	FileMetadataRef m_fileMetadata = nullptr;
	BinaryViewRef m_rawData = nullptr;
	std::vector<std::tuple<std::string, size_t, std::string, uint64_t, uint64_t, std::string>> m_objects;

 public:
	DebugInfoImport(QWidget* parent, BinaryNinja::Ref<BinaryNinja::DebugInfo> debugInfo, BinaryViewRef bv);
	virtual ~DebugInfoImport();

  private Q_SLOTS:
	void cancel();
	void import();
	void addSettingsViewForType(const std::string& bvtName);
	void viewTabCloseRequested(int index);
};
