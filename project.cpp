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
#include "binaryninjacore.h"

using namespace BinaryNinja;


Project::Project(BNProject* project)
{
	m_object = project;
}


Ref<Project> Project::CreateProject(const std::string& path, const std::string& name)
{
	BNProject* bnproj = BNCreateProject(path.c_str(), name.c_str());
	if (!bnproj)
		return nullptr;
	return new Project(BNNewProjectReference(bnproj));
}


Ref<Project> Project::OpenProject(const std::string& path)
{
	BNProject* bnproj = BNOpenProject(path.c_str());
	if (!bnproj)
		return nullptr;
	return new Project(BNNewProjectReference(bnproj));
}


std::string Project::GetPath() const
{
	return BNProjectGetPath(m_object);
}


std::string Project::GetName() const
{
	return BNProjectGetName(m_object);
}


void Project::SetName(const std::string& name)
{
	BNProjectSetName(m_object, name.c_str());
}


bool Project::PathExists(Ref<ProjectFolder> folder, const std::string& name) const
{
	LogWarn("Path exists called");
	return false;
}


Ref<ProjectBinary> Project::GetBinaryById(const std::string& id) const
{
	BNProjectBinary* file = BNProjectGetBinaryById(m_object, id.c_str());
	if (file == nullptr)
		return nullptr;
	return new ProjectBinary(BNNewProjectBinaryReference(file));
}


Ref<ProjectFolder> Project::GetFolderById(const std::string& id) const
{
	BNProjectFolder* folder = BNProjectGetFolderById(m_object, id.c_str());
	if (folder == nullptr)
		return nullptr;
	return new ProjectFolder(BNNewProjectFolderReference(folder));
}


ProjectBinary::ProjectBinary(BNProjectBinary* binary)
{
	m_object = binary;
}


Ref<Project> ProjectBinary::GetProject() const
{
	return new Project(BNProjectBinaryGetProject(m_object));
}


std::string ProjectBinary::GetPathOnDisk() const
{
	return BNProjectBinaryGetPathOnDisk(m_object);
}


std::string ProjectBinary::GetName() const
{
	return BNProjectBinaryGetName(m_object);
}


void ProjectBinary::SetName(const std::string& name)
{
	BNProjectBinarySetName(m_object, name.c_str());
}


std::string ProjectBinary::GetId() const
{
	return BNProjectBinaryGetId(m_object);
}


Ref<ProjectFolder> ProjectBinary::GetFolder() const
{
	BNProjectFolder* folder = BNProjectBinaryGetFolder(m_object);
	if (!folder)
		return nullptr;
	return new ProjectFolder(BNNewProjectFolderReference(folder));
}


void ProjectBinary::SetFolder(Ref<ProjectFolder> folder)
{
	BNProjectBinarySetFolder(m_object, folder ? folder->m_object : nullptr);
}


void ProjectBinary::Delete()
{
	BNProjectBinaryDelete(m_object);
}


void ProjectBinary::Save()
{
	BNProjectBinarySave(m_object);
}


ProjectFolder::ProjectFolder(BNProjectFolder* folder)
{
	m_object = folder;
}


Ref<Project> ProjectFolder::GetProject() const
{
	return new Project(BNProjectFolderGetProject(m_object));
}


std::string ProjectFolder::GetId() const
{
	return BNProjectFolderGetId(m_object);
}


std::string ProjectFolder::GetName() const
{
	return BNProjectFolderGetName(m_object);
}


void ProjectFolder::SetName(const std::string& name)
{
	BNProjectFolderSetName(m_object, name.c_str());
}


Ref<ProjectFolder> ProjectFolder::GetParent() const
{
	BNProjectFolder* parent = BNProjectFolderGetParent(m_object);
	if (!parent)
		return nullptr;
	return new ProjectFolder(BNNewProjectFolderReference(parent));
}


void ProjectFolder::SetParent(Ref<ProjectFolder> parent)
{
	BNProjectFolderSetParent(m_object, parent ? parent->m_object : nullptr);
}


void ProjectFolder::Delete()
{
	BNProjectFolderDelete(m_object);
}


void ProjectFolder::Save()
{
	BNProjectFolderSave(m_object);
}


std::vector<Ref<ProjectFolder>> ProjectFolder::GetFolders() const
{
	size_t count;
	BNProjectFolder** folders = BNProjectFolderGetFolders(m_object, &count);

	std::vector<Ref<ProjectFolder>> result;
	result.reserve(count);
	for (size_t i = 0; i < count; i++)
	{
		result.push_back(new ProjectFolder(BNNewProjectFolderReference(folders[i])));
	}

	BNFreeProjectFolderList(folders, count);
	return result;
}


std::vector<Ref<ProjectBinary>> ProjectFolder::GetBinaries() const
{
	size_t count;
	BNProjectBinary** binaries = BNProjectFolderGetBinaries(m_object, &count);

	std::vector<Ref<ProjectBinary>> result;
	result.reserve(count);
	for (size_t i = 0; i < count; i++)
	{
		result.push_back(new ProjectBinary(BNNewProjectBinaryReference(binaries[i])));
	}

	BNFreeProjectBinaryList(binaries, count);
	return result;
}


Ref<ProjectFolder> ProjectFolder::AddFolder(const std::string& name)
{
	BNProjectFolder* folder = BNProjectFolderAddFolder(m_object, name.c_str());
	if (!folder)
		return nullptr;
	return new ProjectFolder(BNNewProjectFolderReference(folder));
}


Ref<ProjectBinary> ProjectFolder::AddBinary(Ref<FileMetadata> metadata, const std::string &name)
{
	BNProjectBinary* binary = BNProjectFolderAddBinary(m_object, metadata->m_object, name.c_str());
	if (!binary)
		return nullptr;
	return new ProjectBinary(BNNewProjectBinaryReference(binary));
}


Ref<ProjectFolder> Project::AddFolder(Ref<ProjectFolder> parent, const std::string& name)
{
	BNProjectFolder* folder = BNProjectAddFolder(m_object, parent ? parent->m_object : nullptr, name.c_str());
	if (!folder)
		return nullptr;
	return new ProjectFolder(BNNewProjectFolderReference(folder));
}


Ref<ProjectBinary> Project::AddBinary(Ref<FileMetadata> metadata, Ref<ProjectFolder> folder, const std::string &name)
{
	BNProjectBinary* binary = BNProjectAddBinary(m_object, metadata->m_object, folder ? folder->m_object : nullptr, name.c_str());
	if (!binary)
		return nullptr;
	return new ProjectBinary(BNNewProjectBinaryReference(binary));
}


std::vector<Ref<ProjectBinary>> Project::GetTopLevelBinaries() const
{
	size_t count;
	BNProjectBinary** binaries = BNProjectGetTopLevelBinaries(m_object, &count);

	std::vector<Ref<ProjectBinary>> result;
	result.reserve(count);
	for (size_t i = 0; i < count; i++)
	{
		result.push_back(new ProjectBinary(BNNewProjectBinaryReference(binaries[i])));
	}

	BNFreeProjectBinaryList(binaries, count);
	return result;
}


std::vector<Ref<ProjectFolder>> Project::GetTopLevelFolders() const
{
	size_t count;
	BNProjectFolder** folders = BNProjectGetTopLevelFolders(m_object, &count);

	std::vector<Ref<ProjectFolder>> result;
	result.reserve(count);
	for (size_t i = 0; i < count; i++)
	{
		printf("deep in api land %s\n", BNProjectFolderGetId(folders[i]));
		result.push_back(new ProjectFolder(BNNewProjectFolderReference(folders[i])));
	}

	BNFreeProjectFolderList(folders, count);
	return result;
}
