#include "binaryninjaapi.h"
#include <string>
using namespace std;
using namespace BinaryNinja;

bool BinaryNinja::DemangleMS(Architecture* arch, const std::string& mangledName, Type** outType, QualifiedName& outVarName,
    const Ref<BinaryView>& view)
{
	const bool simplify = Settings::Instance()->Get<bool>("analysis.types.templateSimplifier", view);
	return DemangleMS(arch, mangledName, outType, outVarName, simplify);
}

bool BinaryNinja::DemangleMS(Architecture* arch, const std::string& mangledName, Type** outType, QualifiedName& outVarName,
    const bool simplify)
{
	BNType* localType = nullptr;
	char** localVarName = nullptr;
	size_t localSize = 0;
	if (!BNDemangleMS(arch->GetObject(), mangledName.c_str(), &localType, &localVarName, &localSize, simplify))
		return false;
	if (!localType)
		return false;
	*outType = new Type(BNNewTypeReference(localType));
	for (size_t i = 0; i < localSize; i++)
	{
		outVarName.push_back(localVarName[i]);
		BNFreeString(localVarName[i]);
	}
	delete[] localVarName;
	return true;
}

bool BinaryNinja::DemangleGNU3(Ref<Architecture> arch, const std::string& mangledName, Type** outType, QualifiedName& outVarName,
    const Ref<BinaryView>& view)
{
	const bool simplify = Settings::Instance()->Get<bool>("analysis.types.templateSimplifier", view);
	return DemangleGNU3(arch, mangledName, outType, outVarName, simplify);
}

bool BinaryNinja::DemangleGNU3(Ref<Architecture> arch, const std::string& mangledName, Type** outType, QualifiedName& outVarName,
    const bool simplify)
{
	BNType* localType;
	char** localVarName = nullptr;
	size_t localSize = 0;
	if (!BNDemangleGNU3(arch->GetObject(), mangledName.c_str(), &localType, &localVarName, &localSize, simplify))
		return false;
	if (!localType)
		return false;
	*outType = new Type(BNNewTypeReference(localType));
	for (size_t i = 0; i < localSize; i++)
	{
		outVarName.push_back(localVarName[i]);
		BNFreeString(localVarName[i]);
	}
	delete[] localVarName;
	return true;
}


string SimplifyName::to_string(const string& input)
{
	return (string)SimplifyName(input, SimplifierDest::str, true);
}


string SimplifyName::to_string(const QualifiedName& input)
{
	return (string)SimplifyName(input.GetString(), SimplifierDest::str, true);
}


QualifiedName SimplifyName::to_qualified_name(const string& input, bool simplify)
{
	return SimplifyName(input, SimplifierDest::fqn, simplify).operator QualifiedName();
}


QualifiedName SimplifyName::to_qualified_name(const QualifiedName& input)
{
	return SimplifyName(input.GetString(), SimplifierDest::fqn, true).operator QualifiedName();
}


SimplifyName::SimplifyName(const string& input, const SimplifierDest dest, const bool simplify) :
    m_rust_string(nullptr), m_rust_array(nullptr), m_length(0)
{
	if (dest == SimplifierDest::str)
		m_rust_string = BNRustSimplifyStrToStr(input.c_str());
	else
		m_rust_array = const_cast<const char**>(BNRustSimplifyStrToFQN(input.c_str(), simplify));
}


SimplifyName::~SimplifyName()
{
	if (m_rust_string)
		BNRustFreeString(m_rust_string);
	if (m_rust_array)
	{
		if (m_length == 0)
		{
			// Should never reach here
			LogWarn("Deallocating SimplifyName without having been used; Likely misuse of API.\n");
			uint64_t index = 0;
			while (m_rust_array[index][0] != 0x0)
				++index;
			m_length = index + 1;
		}
		BNRustFreeStringArray(m_rust_array, m_length);
	}
}


SimplifyName::operator string() const { return string(m_rust_string); }


SimplifyName::operator QualifiedName()
{
	QualifiedName result;
	uint64_t index = 0;
	while (m_rust_array[index][0] != 0x0)
	{
		result.push_back(string(m_rust_array[index++]));
	}
	m_length = index;
	return result;
}


Demangler::Demangler(const std::string& name): m_nameForRegister(name)
{

}


Demangler::Demangler(BNDemangler* demangler)
{
	m_object = demangler;
}


bool Demangler::IsMangledStringCallback(void* ctxt, const char* name)
{
	Demangler* demangler = (Demangler*)ctxt;
	return demangler->IsMangledString(name);
}


bool Demangler::DemangleCallback(void* ctxt, BNArchitecture* arch, const char* name, BNType** outType,
	BNQualifiedName* outVarName, BNBinaryView* view, bool simplify)
{
	Demangler* demangler = (Demangler*)ctxt;

	Ref<Architecture> apiArch = new CoreArchitecture(arch);
	Ref<BinaryView> apiView = view ? new BinaryView(view) : nullptr;

	Ref<Type> apiType;
	QualifiedName apiVarName;
	bool success = demangler->Demangle(apiArch, name, apiType, apiVarName, apiView, simplify);
	if (!success)
		return false;

	if (apiType)
	{
		apiType->AddRefForRegistration();
		*outType = apiType->m_object;
	}
	*outVarName = apiVarName.GetAPIObject();

	return true;
}


void Demangler::FreeVarNameCallback(void* ctxt, BNQualifiedName* name)
{
	QualifiedName::FreeAPIObject(name);
}


void Demangler::Register(Demangler* demangler)
{
	BNDemanglerCallbacks cb;
	cb.isMangledString = IsMangledStringCallback;
	cb.demangle = DemangleCallback;
	cb.freeVarName = FreeVarNameCallback;
	demangler->m_object = BNRegisterDemangler(demangler->m_nameForRegister.c_str(), &cb);
}


std::vector<Ref<Demangler>> Demangler::GetList()
{
	size_t count;
	BNDemangler** list = BNGetDemanglerList(&count);
	vector<Ref<Demangler>> result;
	for (size_t i = 0; i < count; i++)
		result.push_back(new CoreDemangler(list[i]));
	BNFreeDemanglerList(list);
	return result;
}


Ref<Demangler> Demangler::GetByName(const std::string& name)
{
	BNDemangler* result = BNGetDemanglerByName(name.c_str());
	if (!result)
		return nullptr;
	return new CoreDemangler(result);
}


const std::string& Demangler::GetName() const
{
	return BNGetDemanglerName(m_object);
}


CoreDemangler::CoreDemangler(BNDemangler* demangler): Demangler(demangler)
{

}


bool CoreDemangler::IsMangledString(const std::string& name)
{
	return BNIsDemanglerMangledName(m_object, name.c_str());
}


bool CoreDemangler::Demangle(Ref<Architecture> arch, const std::string& name, Ref<Type>& outType,
	QualifiedName& outVarName, Ref<BinaryView> view, bool simplify)
{
	BNType* apiType;
	BNQualifiedName apiVarName;
	bool success = BNDemanglerDemangle(
		m_object, arch->m_object, name.c_str(), &apiType, &apiVarName, view ? view->m_object : nullptr, simplify);

	if (!success)
		return false;

	if (apiType)
		outType = new Type(apiType);
	outVarName = QualifiedName::FromAPIObject(&apiVarName);
	BNFreeQualifiedName(&apiVarName);
	return true;
}
