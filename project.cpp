// Copyright (c) 2015-2023 Vector 35 Inc
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
#include <cstring>
#include "binaryninjaapi.h"

using namespace BinaryNinja;


Project::Project(BNProject* project)
{
	m_object = project;
}


Ref<Project> Project::CreateProject(const std::string& path)
{
	BNProject* bnproj = BNCreateProject(path.c_str());
	if (!bnproj)
		return nullptr;
	return new Project(bnproj);
}


Ref<Project> Project::OpenProject(const std::string& path)
{
	BNProject* bnproj = BNOpenProject(path.c_str());
	if (!bnproj)
		return nullptr;
	return new Project(bnproj);
}


Ref<ProjectFile> Project::AddFile(const std::string& srcPath, Ref<ProjectFolder> folder, const std::string& name)
{
	BNProjectFile* file = BNProjectAddFile(m_object, srcPath.c_str(), folder ? folder->m_object : nullptr, name.c_str());
	if (file == nullptr)
		return nullptr;
	return new ProjectFile(file);
}


Ref<ProjectFolder> Project::AddFolder(Ref<ProjectFolder> parent, const std::string& name)
{
	BNProjectFolder* folder = BNProjectAddFolder(m_object, parent ? parent->m_object : nullptr, name.c_str());
	if (folder == nullptr)
		return nullptr;
	return new ProjectFolder(folder);
}


std::string Project::GetPath() const
{
	return BNProjectGetPath(m_object);
}


std::vector<Ref<ProjectFile>> Project::GetFiles() const
{
	std::vector<Ref<ProjectFile>> out;

	size_t count = 0;
	BNProjectFile** files = BNProjectGetFiles(m_object, &count);
	std::vector<Ref<ProjectFile>> result;
	for (size_t i = 0; i < count; i++)
	{
		BNProjectFile* temp = files[i];
		result.push_back(new ProjectFile(BNNewProjectFileReference(temp)));

	}
	BNFreeProjectFileList(files, count);
	return result;
}


Ref<ProjectFile> Project::GetFileById(const std::string& id) const
{
	BNProjectFile* file = BNProjectGetFileById(m_object, id.c_str());
	if (file == nullptr)
		return nullptr;
	return new ProjectFile(file);
}


std::vector<Ref<ProjectFolder>> Project::GetFolders() const
{
	std::vector<Ref<ProjectFolder>> out;

	size_t count = 0;
	BNProjectFolder** folders = BNProjectGetFolders(m_object, &count);
	std::vector<Ref<ProjectFolder>> result;
	for (size_t i = 0; i < count; i++)
	{
		BNProjectFolder* temp = folders[i];
		result.push_back(new ProjectFolder(BNNewProjectFolderReference(temp)));

	}
	BNFreeProjectFolderList(folders, count);
	return result;
}


std::vector<Ref<ProjectFolder>> Project::GetSortedFolders() const
{
	auto sortedFolders = GetFolders();
	std::sort(sortedFolders.begin(), sortedFolders.end(), [](const Ref<ProjectFolder>& lhs, const Ref<ProjectFolder>& rhs) {
		// ensure strict weak ordering because that's VERY IMPORTANT
		if (lhs == rhs)
			return false;

		if (!lhs)
			throw ProjectException("Failed to sort folders, lhs is null");
		if (!rhs)
			throw ProjectException("Failed to sort folders, rhs is null");

		auto lhsParent = lhs->GetParent();
		auto rhsParent = rhs->GetParent();
		if (!lhsParent)
		{
			if (rhsParent)
			{
				// Left is root, right is not root
				return true;
			}
			else
			{
				// Left is root, right is root
				return lhs->GetId() < rhs->GetId();
			}
		}
		else if (!rhsParent)
		{
			// Left is not root, right is root
			return false;
		}

		// Left is not root, right is not root

		// check if rhs is in parent tree of lhs
		while (lhsParent)
		{
			if (lhsParent->GetId() == rhs->GetId())
			{
				return false;
			}
			lhsParent = lhsParent->GetParent();
		}
		return true;
	});
	return sortedFolders;
}


Ref<ProjectFolder> Project::GetFolderById(const std::string& id) const
{
	BNProjectFolder* folder = BNProjectGetFolderById(m_object, id.c_str());
	if (folder == nullptr)
		return nullptr;
	return new ProjectFolder(folder);
}


ProjectFile::ProjectFile(BNProjectFile* file)
{
	m_object = file;
}


std::string ProjectFile::GetPath() const
{
	return BNProjectFileGetPath(m_object);
}


std::string ProjectFile::GetName() const
{
	return BNProjectFileGetName(m_object);
}


std::string ProjectFile::GetId() const
{
	return BNProjectFileGetId(m_object);
}


Ref<ProjectFolder> ProjectFile::GetFolder() const
{
	BNProjectFolder* folder = BNProjectFileGetFolder(m_object);
	if (!folder)
		return nullptr;
	return new ProjectFolder(folder);
}


void ProjectFile::SetFolder(Ref<ProjectFolder> folder)
{
	BNProjectFileSetFolder(m_object, folder ? folder->m_object : nullptr);
}


ProjectFolder::ProjectFolder(BNProjectFolder* folder)
{
	m_object = folder;
}


std::string ProjectFolder::GetId() const
{
	return BNProjectFolderGetId(m_object);
}


std::string ProjectFolder::GetName() const
{
	return BNProjectFolderGetName(m_object);
}


Ref<ProjectFolder> ProjectFolder::GetParent() const
{
	BNProjectFolder* parent = BNProjectFolderGetParent(m_object);
	if (!parent)
		return nullptr;
	return new ProjectFolder(parent);
}


void ProjectFolder::SetParent(Ref<ProjectFolder> parent)
{
	BNProjectFolderSetParent(m_object, parent ? parent->m_object : nullptr);
}
