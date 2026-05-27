import sys

from ._types import CellLibraryEntry, CellLibraryMetadata, CellLibrarySummary


_api = sys.modules[__package__]


class Pdk:
    def __init__(self, core) -> None:
        if core is None:
            raise ValueError("Pdk core object must not be None")
        self._core = core

    def __repr__(self) -> str:
        version = self.cell_library_version
        version_suffix = "" if version is None else f", version={version!r}"
        return f"Pdk(name={self.name!r}, cell_library={self.cell_library_name!r}{version_suffix})"

    @classmethod
    def minimal(cls, name: str = "py-minimal-pdk") -> "Pdk":
        """Create the built-in minimal PDK."""
        _api._require_core_extension("Pdk.minimal(...)", _api._CorePdk)
        return cls(_api._CorePdk.minimal(name))

    @classmethod
    def from_json(cls, payload: str) -> "Pdk":
        """Load a PDK from its JSON representation."""
        _api._require_core_extension("Pdk.from_json(...)", _api._CorePdk)
        return cls(_api._CorePdk.from_json(payload))

    @property
    def name(self) -> str:
        """PDK name."""
        return self._core.name

    def to_json(self) -> str:
        """Serialize this PDK to JSON."""
        return self._core.to_json()

    @property
    def active_timing_corner(self) -> str | None:
        """Name of the selected timing corner, when one is active."""
        return self._core.active_timing_corner

    def timing_corner_names(self) -> list[str]:
        """Return the available timing corner names."""
        return list(self._core.timing_corner_names())

    def with_active_timing_corner(self, name: str) -> "Pdk":
        """Return a PDK selecting ``name`` as the active timing corner."""
        return Pdk(self._core.with_active_timing_corner(name))

    @property
    def cell_library_name(self) -> str:
        """Name of the active cell library."""
        return self._core.cell_library_name

    @property
    def cell_library_version(self) -> str | None:
        """Version of the active cell library, when available."""
        return self._core.cell_library_version

    @property
    def cell_library_source(self) -> str | None:
        """Source label or path for the active cell library, when available."""
        return self._core.cell_library_source

    def cell_library_metadata(self) -> CellLibraryMetadata:
        """Return metadata for the active cell library."""
        return _api._cell_library_metadata_from_core(self._core.cell_library_metadata())

    def cell_library_kinds(self) -> list[str]:
        """Return the cell kinds represented by the active library."""
        return list(self._core.cell_library_kinds())

    def cell_library_entries(self) -> list[CellLibraryEntry]:
        """Return all active cell library entries."""
        return [
            _api._cell_library_entry_from_core(entry)
            for entry in self._core.cell_library_entries()
        ]

    def cell_library_summary(self) -> CellLibrarySummary:
        """Return a compact summary of active cell library coverage."""
        return _api._cell_library_summary_from_core(self._core.cell_library_summary())

    def cell_library_entries_by_kind(self, kind: str) -> list[CellLibraryEntry]:
        """Return active cell library entries matching ``kind``."""
        return [
            _api._cell_library_entry_from_core(entry)
            for entry in self._core.cell_library_entries_by_kind(kind)
        ]

    def cell_library_entry(self, cell_name: str) -> CellLibraryEntry | None:
        """Return a named cell library entry, if present."""
        entry = self._core.cell_library_entry(cell_name)
        return None if entry is None else _api._cell_library_entry_from_core(entry)

    def merge_characterized_library_json(self, serialized_entry: str) -> "Pdk":
        """Return a PDK with one characterized cell entry merged in."""
        return Pdk(self._core.merge_characterized_library_json(serialized_entry))

    def merge_characterized_library_entries(self, serialized_entries: list[str]) -> "Pdk":
        """Return a PDK with multiple characterized cell entries merged in."""
        return Pdk(self._core.merge_characterized_library_entries(serialized_entries))

__all__ = [
    "Pdk",
    "CellLibraryEntry",
    "CellLibraryMetadata",
    "CellLibrarySummary",
]