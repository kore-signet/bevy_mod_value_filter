use core::{cell::UnsafeCell, marker::PhantomData};

use bevy_ecs::{
    archetype::Archetype,
    component::{Component, ComponentId, Components, StorageType, Tick},
    entity::Entity,
    ptr::{ThinSlicePtr, UnsafeCellDeref},
    query::{FilteredAccess, QueryFilter, WorldQuery},
    storage::{ComponentSparseSet, Table, TableRow},
    world::unsafe_world_cell::UnsafeWorldCell,
};

use crate::{Check, Predicate};

pub struct CheckFetch<'w, T, Pred> {
    pred_marker: PhantomData<Pred>,

    // T::STORAGE_TYPE == TableStorage
    table_components: Option<ThinSlicePtr<'w, UnsafeCell<T>>>,
    // entity_table_rows: ThinSlicePtr<'w, usize>,

    // T::STORAGE_TYPE == SparseStorage
    entities: Option<ThinSlicePtr<'w, Entity>>,
    sparse_set: Option<&'w ComponentSparseSet>,
}

impl<T, Pred> Clone for CheckFetch<'_, T, Pred> {
    fn clone(&self) -> Self {
        Self {
            pred_marker: PhantomData,
            table_components: self.table_components,
            entities: self.entities,
            sparse_set: self.sparse_set,
        }
    }
}

pub struct CheckState<T> {
    component_id: ComponentId,
    marker: PhantomData<T>,
}

unsafe impl<T: Component, Pred: Predicate<T>> WorldQuery for Check<T, Pred> {
    type State = CheckState<T>;
    type Fetch<'w> = CheckFetch<'w, T, Pred>;

    const IS_DENSE: bool = match T::STORAGE_TYPE {
        StorageType::Table => true,
        StorageType::SparseSet => false,
    };

    fn init_state(world: &mut bevy_ecs::world::World) -> Self::State {
        CheckState {
            component_id: world.register_component::<T>(),
            marker: PhantomData,
        }
    }

    unsafe fn init_fetch<'w>(
        world: UnsafeWorldCell<'w>,
        state: &Self::State,
        _last_run: Tick,
        _this_run: Tick,
    ) -> Self::Fetch<'w> {
        CheckFetch {
            pred_marker: PhantomData,
            table_components: None,
            entities: None,
            sparse_set: (T::STORAGE_TYPE == StorageType::SparseSet).then(|| unsafe {
                world
                    .storages()
                    .sparse_sets
                    .get(state.component_id)
                    .unwrap()
            }),
        }
    }

    fn shrink_fetch<'wlong: 'wshort, 'wshort>(fetch: Self::Fetch<'wlong>) -> Self::Fetch<'wshort> {
        fetch
    }

    unsafe fn set_archetype<'w>(
        fetch: &mut Self::Fetch<'w>,
        state: &Self::State,
        _archetype: &'w Archetype,
        table: &'w Table,
    ) {
        if Self::IS_DENSE {
            unsafe {
                Self::set_table(fetch, state, table);
            }
        }
    }

    unsafe fn set_table<'w>(fetch: &mut Self::Fetch<'w>, state: &Self::State, table: &'w Table) {
        fetch.table_components = Some(unsafe {
            table
                .get_column(state.component_id)
                .unwrap()
                .get_data_slice(table.entity_count())
                .into()
        })
    }

    fn get_state(components: &Components) -> Option<Self::State> {
        components
            .component_id::<T>()
            .map(|component_id| CheckState {
                component_id,
                marker: PhantomData,
            })
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        set_contains_id(state.component_id)
    }

    fn update_component_access(state: &Self::State, access: &mut FilteredAccess<ComponentId>) {
        assert!(
            !access.access().has_component_write(state.component_id),
            "Equals<{}, _> conflicts with a previous access in this query. Shared access cannot coincide with exclusive access.",
            core::any::type_name::<T>(),
        );

        access.add_component_read(state.component_id);
    }
}

unsafe impl<T: Component, Pred: Predicate<T>> QueryFilter for Check<T, Pred> {
    const IS_ARCHETYPAL: bool = false;

    unsafe fn filter_fetch(
        fetch: &mut Self::Fetch<'_>,
        _entity: Entity,
        table_row: TableRow,
    ) -> bool {
        let item = unsafe {
            fetch
                .table_components
                .unwrap()
                .get(table_row.as_usize())
                .deref()
        };

        Pred::test(item)
    }
}
