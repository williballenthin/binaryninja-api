#pragma once

#include <QtWidgets/QDialog>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QCheckBox>
#include <QtWidgets/QComboBox>
#include <QtWidgets/QTextEdit>
#include <QtWidgets/QPushButton>
#include "binaryninjaapi.h"
#include "uicontext.h"

class BINARYNINJAUIAPI CreateArrayDialog : public QDialog
{
	Q_OBJECT

	QComboBox* m_mode;
	QLineEdit* m_type, *m_size;
	QLabel* m_typeLabel, *m_sizeLabel;
	QTextEdit* m_errors;
	QPushButton* m_acceptButton;

	BinaryViewRef m_view;
	BinaryNinja::Ref<BinaryNinja::Type> m_resultType;

public:
	using CursorPositions = std::pair<LinearViewCursorPosition, LinearViewCursorPosition>;

	enum Mode : uint8_t
	{
		FillToSize = 0,
		FillToSizeWithType,
		FillToEndOfSection,
		FillToNextDataVariable,
	};

	CreateArrayDialog(QWidget* parent, BinaryViewRef view, const CursorPositions& cursorPositions,
		Mode initialMode = Mode::FillToSize);

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

private:
	void resetLabels();
	void setContent(const CursorPositions& cursorPositions);
	void accepted();
	void indexChanged(int);
};
