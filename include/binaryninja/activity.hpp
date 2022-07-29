#pragma once

#include <string>
#include <vector>
#include <variant>
#include "json/json.h"
#include "binaryninjacore/activity.h"
#include "confidence.hpp"
#include "architecture.hpp"

namespace BinaryNinja
{
	class Architecture;
	class BasicBlock;
	class Function;
	class LowLevelILFunction;
	class MediumLevelILFunction;
	class HighLevelILFunction;
	class DelegateInterface;
	class AnalysisContext;

	class AnalysisContext :
		public CoreRefCountObject<BNAnalysisContext, BNNewAnalysisContextReference, BNFreeAnalysisContext>
	{
		std::unique_ptr<Json::CharReader> m_reader;
		Json::StreamWriterBuilder m_builder;

	  public:
		AnalysisContext(BNAnalysisContext* analysisContext);
		virtual ~AnalysisContext();

		Ref<Function> GetFunction();
		Ref<LowLevelILFunction> GetLowLevelILFunction();
		Ref<MediumLevelILFunction> GetMediumLevelILFunction();
		Ref<HighLevelILFunction> GetHighLevelILFunction();

		void SetBasicBlockList(std::vector<Ref<BasicBlock>> basicBlocks);
		void SetLiftedILFunction(Ref<LowLevelILFunction> liftedIL);
		void SetLowLevelILFunction(Ref<LowLevelILFunction> lowLevelIL);
		void SetMediumLevelILFunction(Ref<MediumLevelILFunction> mediumLevelIL);
		void SetHighLevelILFunction(Ref<HighLevelILFunction> highLevelIL);

		bool Inform(const std::string& request);
	#if ((__cplusplus >= 201403L) || (_MSVC_LANG >= 201703L))
		template <class... Ts>
		struct overload : Ts...
		{
			using Ts::operator()...;
		};
		template <class... Ts>
		overload(Ts...) -> overload<Ts...>;

		template <typename... Args>
		bool Inform(Args... args)
		{
			// using T = std::variant<Args...>; // FIXME: remove type duplicates
			using T = std::variant<std::string, const char*, uint64_t, Ref<Architecture>>;
			std::vector<T> unpackedArgs {args...};
			Json::Value request(Json::arrayValue);
			for (auto& arg : unpackedArgs)
				std::visit(overload {[&](Ref<Architecture> arch) { request.append(Json::Value(arch->GetName())); },
								[&](uint64_t val) { request.append(Json::Value(val)); },
								[&](auto& val) {
									request.append(Json::Value(std::forward<decltype(val)>(val)));
								}},
					arg);

			return Inform(Json::writeString(m_builder, request));
		}
	#endif
	};

	class Activity : public CoreRefCountObject<BNActivity, BNNewActivityReference, BNFreeActivity>
	{
	  protected:
		std::function<void(Ref<AnalysisContext> analysisContext)> m_action;

		static void Run(void* ctxt, BNAnalysisContext* analysisContext);

	  public:
		Activity(const std::string& name, const std::function<void(Ref<AnalysisContext>)>& action);
		Activity(BNActivity* activity);
		virtual ~Activity();

		std::string GetName() const;
	};
}