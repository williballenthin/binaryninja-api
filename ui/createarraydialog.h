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

	QComboBox* m_mode;
	QLineEdit* m_type, *m_size, *m_address, *m_startAddress;
	QLabel* m_typeLabel, *m_sizeLabel, *m_addressLabel, *m_startAddressLabel;
	QTextEdit* m_errors;
	QPushButton* m_acceptButton;
	QListWidget* m_dataVariableList;

	BinaryViewRef m_view;
	BinaryNinja::Ref<BinaryNinja::Type> m_resultType;
	uint64_t m_highestAddress, m_lowestAddress;
	bool m_sizeMismatch{false};
	std::vector<BinaryNinja::DataVariable> m_dataVariables;

public:
	using CursorPositions = std::pair<LinearViewCursorPosition, LinearViewCursorPosition>;

	enum Mode : uint8_t
	{
		Default = 0,
		FillToDataVariable,
	};

	CreateArrayDialog(QWidget* parent, BinaryViewRef view, const CursorPositions& cursorPositions,
		std::vector<BinaryNinja::DataVariable> dataVariables, Mode initialMode = Mode::Default);

	BinaryNinja::Ref<BinaryNinja::Type> getType() { return m_resultType; }

	Mode getMode() { return static_cast<Mode>(m_mode->currentIndex()); }

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

	std::optional<BinaryNinja::DataVariable> getSelectedDataVariable()
	{
		if (const auto item = m_dataVariableList->currentItem())
			return m_dataVariables.at(m_dataVariableList->currentIndex().row());

		return std::nullopt;
	}

private:
	void sizeChanged(const QString& size);
	void addressChanged(const QString& address);
	void typeChanged(const QString& type);

	void itemSelectionChanged();
	void resetLabels();
	void updateDataVariables();
	void accepted();
	void indexChanged(int);
};
