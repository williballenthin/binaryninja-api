#pragma once

#include <QtWidgets/QDialog>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QTextEdit>
#include <QtWidgets/QPushButton>
#include <QtWidgets/QListWidget>
#include "binaryninjaapi.h"
#include "uicontext.h"

class BINARYNINJAUIAPI CreateArrayDialog : public QDialog
{
	Q_OBJECT

	QLineEdit *m_type, *m_size, *m_address;
	QLabel *m_typeLabel, *m_sizeLabel, *m_addressLabel, *m_startAddress, *m_errors;
	QPushButton* m_acceptButton;

	BinaryViewRef m_view;
	BinaryNinja::Ref<BinaryNinja::Type> m_resultType;
	uint64_t m_highestAddress, m_lowestAddress, m_count;

public:
	CreateArrayDialog(QWidget* parent, BinaryViewRef view, uint64_t startAddress, uint64_t endAddress);

	BinaryNinja::Ref<BinaryNinja::Type> getType() { return m_resultType; }

	size_t getSize()
	{
		bool ok{false};
		const auto sz = m_size->text().toULongLong(&ok);
		if (ok)
			return sz;
		return 0;
	}

	uint64_t getAddress()
	{
		bool ok{false};
		const auto sz = m_address->text().toULongLong(&ok, 16);
		if (ok)
			return sz;
		return 0;
	}

private:
	void sizeChanged(const QString& sizeText);
	void addressChanged(const QString& addressText);
	void typeChanged(const QString& typeText);

	bool validate();
	void accepted();
};
