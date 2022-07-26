
import ctypes
import inspect
from typing import Generator, Optional, List, Tuple, Union, Mapping, Any, Dict, Iterator
from dataclasses import dataclass

from . import binaryview

from . import function
from . import _binaryninjacore as core
from binaryninja.types import Type

"""
from typing import Iterable
import traceback

class ScriptBasedTestRunner:
    def __init__(self) -> None:
        self.bv = bv

    def assertEqual(self, a, b, message=""):
        assert a == b, message

    def assertContains(self, haystack, needle):
        assert needle in haystack

    def assertLenEqual(self, a: Iterable, size, message=""):
        assert len(a) == size, message

    def assertLenGreater(self, a: Iterable, minimum_size, message=""):
        assert len(a) > minimum_size, message

    @classmethod
    def run_tests(cls):

        print(f'--------\n{cls.__name__}\n--------')

        method_list = [func for func in dir(cls) if callable(getattr(cls, func)) and func.startswith('test')]
        
        jobs = []
        inst = cls()
        for meth in method_list:
            func = getattr(inst, meth)
            try:
                func()
                jobs.append(('PASS', ''))
            except AssertionError as ex:
                jobs.append(('FAIL', f'{cls.__name__}.{meth}: {str(ex) if str(ex) != "" else traceback.format_exc()}'))

        total_count = 0
        pass_count = 0
        fails = []
        for job in jobs:
            if job[0] == 'PASS':
                pass_count += 1
            else:
                fails.append(job)
            total_count += 1
        print(f'{pass_count}/{total_count} PASSED')
        for fail in fails:
            print(f'{fail[0]}: {fail[1]}', file=sys.stderr)



class ComponentTests(ScriptBasedTestRunner):
    def __init__(self) -> None:
        super().__init__()

    def test_bv_components(self):
        c = Component(self.bv)
        guid = c.guid
        c.name = "ACoolName"
        self.assertEqual(c.name, "ACoolName")
        f = self.bv.get_functions_by_name('main')[0]
        c.add_function(f)

        assert self.bv.add_component(c)
        self.assertLenGreater(list(self.bv.components), 0)
        assert self.bv.remove_component(c)
        self.assertLenEqual(list(self.bv.components), 0)
        assert self.bv.add_component(c)
        for co in self.bv.components:
            assert self.bv.remove_component_by_guid(co.guid)
        self.assertLenEqual(list(self.bv.components), 0)


    def test_components(self):

        c = Component(self.bv)
        guid = c.guid
        c.name = "ACoolName"
        self.assertEqual(c.name, "ACoolName")
        f = self.bv.get_functions_by_name('main')[0]
        c.remove_function(f)
        c.add_function(f)

        self.assertLenGreater(c.get_referenced_data_variables(), 0)
        self.assertLenGreater(c.get_referenced_types(), 0)
        assert c.contains_function(f)

        assert c.remove_function(f)
        self.assertLenEqual(c.get_referenced_data_variables(), 0)
        self.assertLenEqual(c.get_referenced_types(), 0)
        assert not c.contains_function(f)

        c.add_function(f)

        pC = Component(self.bv)
        pC.add_component(c)
        assert pC.contains_component(c)
        ppC = Component(self.bv)
        ppC.add_function(f)
        ppC.add_component(pC)
        assert pC.parent == ppC
        assert pC.parent != c

        assert self.bv.get_component(pC.guid) == pC

        print(ppC.parent)

        print(self.sprawl_component(ppC))

        pC.remove_component(c)
        assert not pC.contains_component(c)

    def sprawl_component(self, c, depth=1, out=None):
        _out = ([repr(c)] if not out else out.split('\n')) + [('  ' * depth + repr(f)) for f in c.functions]
        _out += ['  ' * (depth+1) + repr(i) for i in (c.get_referenced_data_variables() + c.get_referenced_types())]
        for i in c.components:
            _out.append('  ' * depth + repr(i))
            _out = self.sprawl_component(i, depth+1, '\n'.join(_out)).split('\n')
        return '\n'.join(_out)

ComponentTests.run_tests()

"""

class Component:
    def __init__(self, view=None, handle=None):

        assert view is not None, "Component must have an attached BinaryView"
        assert handle is not None, "Cannot create component directly, run `bv.create_component?`"

        self.view: 'binaryview.BinaryView' = view
        self.handle = handle

        self.guid = core.BNComponentGetGuid(self.handle)

    def __eq__(self, other):
        if not isinstance(other, Component):
            return NotImplemented
        return core.BNComponentsEqual(self.handle, other.handle)

    def __ne__(self, other):
        if not isinstance(other, Component):
            return NotImplemented
        return core.BNComponentsNotEqual(self.handle, other.handle)

    def __repr__(self):
        return f'<Component "{self.name}" "({self.guid[:8]}...")>'

    def __del__(self):
        core.BNFreeComponent(self.handle)

    def __str__(self):
        return self._sprawl_component(self)

    def _sprawl_component(self, c, depth=1, out=None):
        """
        Recursive quick function to print out the component's tree of items

        :param c: Current cycle's component. On initial call, pass `self`
        :param depth: Current tree depth.
        :param out: Current text
        :return:
        """
        _out = ([repr(c)] if not out else out.split('\n')) + [('  ' * depth + repr(f)) for f in c.functions]
        _out += ['  ' * (depth+1) + repr(i) for i in (c.get_referenced_data_variables() + c.get_referenced_types())]
        for i in c.components:
            _out.append('  ' * depth + repr(i))
            _out = self._sprawl_component(i, depth+1, '\n'.join(_out)).split('\n')
        return '\n'.join(_out)

    def add_function(self, func: function.Function) -> bool:
        return core.BNComponentAddFunctionReference(self.handle, func.handle)

    def contains_function(self, func: function.Function) -> bool:
        return core.BNComponentContainsFunction(self.handle, func.handle)

    def remove_function(self, func: function.Function) -> bool:
        return core.BNComponentRemoveFunctionReference(self.handle, func.handle)

    def contains_component(self, component: 'Component') -> bool:
        return core.BNComponentContainsComponent(self.handle, component.handle)

    @property
    def name(self) -> str:
        """Display name of the component"""
        return core.BNComponentGetName(self.handle)

    @name.setter
    def name(self, _name):
        core.BNComponentSetName(self.handle, _name)

    @property
    def parent(self) -> Optional['Component']:
        """
        The component that contains this component, if it exists.
        """
        bn_component = core.BNComponentGetParent(self.handle)
        if bn_component is not None:
            return Component(self.view, bn_component)
        return None

    @property
    def components(self) -> Iterator['Component']:
        """
		``components`` is an iterator for all Components contained within this Component

		:return: An iterator containing Components
		:rtype: SubComponentIterator
		:Example:

			>>> for subcomp in component.components:
			...  print(repr(component))
        """

        @dataclass
        class SubComponentIterator:
            view: 'binaryview.BinaryView'
            comp: Component

            def __iter__(self):
                count = ctypes.c_ulonglong(0)
                bn_components = core.BNComponentGetContainedComponents(self.comp.handle, count)
                for i in range(count.value):
                    yield Component(self.view, bn_components[i])

        return iter(SubComponentIterator(self.view, self))

    @property
    def functions(self) -> Iterator['Component']:
        """
		``functions`` is an iterator for all Functions contained within this Component

		:return: An iterator containing Components
		:rtype: ComponentIterator
		:Example:

			>>> for func in component.functions:
			...  print(func.name)
        """
        @dataclass
        class FunctionIterator:
            view: 'binaryview.BinaryView'
            comp: Component

            def __iter__(self):
                count = ctypes.c_ulonglong(0)
                bn_functions = core.BNComponentGetContainedFunctions(self.comp.handle, count)
                for i in range(count.value):
                    bn_function = bn_functions[i]
                    yield function.Function(self.view, bn_function)

        return iter(FunctionIterator(self.view, self))

    def get_referenced_data_variables(self, recursive=False):
        data_vars = []
        count = ctypes.c_ulonglong(0)
        if recursive:
            bn_data_vars = core.BNComponentGetReferencedDataVariablesRecursive(self.handle, count)
        else:
            bn_data_vars = core.BNComponentGetReferencedDataVariables(self.handle, count)
        try:
            for i in range(count.value):
                bn_data_var = bn_data_vars[i]
                data_var = binaryview.DataVariable.from_core_struct(bn_data_var, self.view)
                data_vars.append(data_var)
        finally:
            core.BNFreeDataVariables(bn_data_vars, count.value)
        return data_vars

    def get_referenced_types(self, recursive=False):
        types = []
        count = ctypes.c_ulonglong(0)

        if recursive:
            bn_types = core.BNComponentGetReferencedTypesRecursive(self.handle, count)
        else:
            bn_types = core.BNComponentGetReferencedTypes(self.handle, count)

        for i in range(count.value):
            types.append(Type(bn_types[i]))

        return types
