export default {
    root: ({ props }) => ({
        class: [
            'relative',

            // Flex & Alignment
            { 'flex flex-col': props.scrollable && props.scrollHeight === 'flex' },

            // Size
            { 'h-full': props.scrollable && props.scrollHeight === 'flex' },
        ],
    }),
    mask: {
        class: [
            // Position
            'absolute',
            'top-0 left-0',
            'z-20',

            // Flex & Alignment
            'flex items-center justify-center',

            // Size
            'w-full h-full',

            // Color
            'bg-bg-two',

            // Transition
            'transition duration-200',
        ],
    },
    loadingIcon: {
        class: 'w-8 h-8 animate-spin',
    },
    tableContainer: ({ props }) => ({
        class: [
            {
                relative: props.scrollable,
                'flex flex-col grow': props.scrollable && props.scrollHeight === 'flex',
            },

            // Size
            { 'h-full': props.scrollable && props.scrollHeight === 'flex' },
        ],
    }),
    header: ({ props }) => ({
        class: [
            'font-bold',

            // Shape
            props.showGridlines ? 'border-x border-t border-b-0' : 'border-y border-x-0',

            // Spacing
            'p-4',

            // Color
            'bg-bg-two',
            'border-border-one',
            'text-text-color',
        ],
    }),
    table: {
        class: 'w-full border-spacing-0 border-separate',
    },
    thead: ({ context }) => ({
        class: [
            {
                'bg-bg-two top-0 z-40 sticky': context.scrollable,
            },
        ],
    }),
    tbody: ({ instance, context }) => ({
        class: [
            {
                'sticky z-20': instance.frozenRow && context.scrollable,
            },
            'bg-bg-one',
        ],
    }),
    tfoot: ({ context }) => ({
        class: [
            {
                'bg-bg-two bottom-0 z-0': context.scrollable,
            },
        ],
    }),
    footer: {
        class: [
            'font-bold',

            // Shape
            'border-t-0 border-b border-x-0',

            // Spacing
            'p-4',

            // Color
            'bg-bg-two',
            'border-border-one',
            'text-text-color',
        ],
    },
    column: {
        headerCell: ({ context, props }) => ({
            class: [
                'font-bold',

                // Position
                { 'sticky z-20 border-b': props.frozen || props.frozen === '' },

                { relative: context.resizable },

                // Alignment
                'text-left',

                // Shape
                { 'first:border-l border-y border-r': context?.showGridlines },
                'border-0 border-b border-solid',

                // Spacing
                context?.size === 'small' ? 'p-2' : context?.size === 'large' ? 'p-5' : 'p-4',

                // Color
                (props.sortable === '' || props.sortable) && context.sorted
                    ? 'bg-highlight'
                    : 'bg-bg-one text-text-color',
                'border-border-one',

                // States
                {
                    'hover:bg-surface-hover':
                        (props.sortable === '' || props.sortable) && !context?.sorted,
                },
                'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-inset focus-visible:ring-primary-400/50',

                // Transition
                { 'transition duration-200': props.sortable === '' || props.sortable },

                // Misc
                { 'cursor-pointer': props.sortable === '' || props.sortable },
                {
                    'overflow-hidden space-nowrap border-y bg-clip-padding': context.resizable, // Resizable
                },
            ],
        }),
        columnHeaderContent: {
            class: 'flex items-center',
        },
        sort: ({ context }) => ({
            class: [context.sorted ? 'text-text-color-secondary' : 'text-text-color'],
        }),
        bodyCell: ({ props, context, state, parent }) => ({
            class: [
                //Position
                { 'sticky box-border border-b': parent.instance.frozenRow },
                { 'sticky box-border border-b z-20': props.frozen || props.frozen === '' },

                // Alignment
                'text-left',

                // Shape
                'border-0 border-b border-solid',
                { 'first:border-l border-r border-b': context?.showGridlines },
                {
                    'bg-surface-0':
                        parent.instance.frozenRow || props.frozen || props.frozen === '',
                },

                // Spacing
                { 'p-2': context?.size === 'small' && !state['d_editing'] },
                { 'p-5': context?.size === 'large' && !state['d_editing'] },
                {
                    'p-4':
                        context?.size !== 'large' &&
                        context?.size !== 'small' &&
                        !state['d_editing'],
                },
                { 'py-[0.6rem] px-2': state['d_editing'] },

                // Color
                'border-border-one',

                {
                    'overflow-hidden whitespace-nowrap border-y bg-clip-padding':
                        parent.instance?.$parentInstance?.$parentInstance?.resizableColumns, // Resizable
                },
            ],
        }),
        footerCell: ({ context }) => ({
            class: [
                // Font
                'font-bold',

                // Alignment
                'text-left',

                // Shape
                'border-0 border-b border-solid',
                { 'border-x border-y': context?.showGridlines },

                // Spacing
                context?.size === 'small' ? 'p-2' : context?.size === 'large' ? 'p-5' : 'p-4',

                // Color
                'border-border-one',
                'text-text-color',
                'bg-bg-two',
            ],
        }),
        sortIcon: ({ context }) => ({
            class: ['ml-2', context.sorted ? 'text-inherit' : 'text-text-color'],
        }),
        columnFilter: {
            class: 'inline-flex items-center ml-auto',
        },
        filterOverlay: {
            class: [
                // Position
                'absolute top-0 left-0',

                // Shape
                'border-0',
                'rounded-lg',
                'shadow-lg',

                // Size
                'min-w-[12.5rem]',

                // Color
                'bg-bg-one',
                'text-text-color',
            ],
        },
        filterConstraintList: {
            class: 'm-0 p-0 py-3 list-none',
        },
        filterConstraint: ({ context }) => ({
            class: [
                // Font
                'font-normal',
                'leading-none',

                // Position
                'relative',

                // Shape
                'border-0',
                'rounded-none',

                // Spacing
                'm-0',
                'py-3 px-5',

                // Color
                {
                    'bg-bg-one text-text-color-secondary': !context?.highlighted,
                },
                { 'bg-surface-hover': context?.highlighted },

                //States
                { 'hover:bg-surface-hover': !context?.highlighted },
                {
                    'hover:text-text-color hover:bg-surface-hover': !context?.highlighted,
                },
                'focus-visible:outline-none focus-visible:outline-offset-0 focus-visible:ring focus-visible:ring-inset focus-visible:ring-primary-400/50',

                // Transitions
                'transition-shadow',
                'duration-200',

                // Misc
                'cursor-pointer',
                'overflow-hidden',
                'whitespace-nowrap',
            ],
        }),
        filterOperator: {
            class: [
                // Shape
                'border-b border-solid',
                'rounded-t-lg',

                // Color
                'text-text-color',
                'border-border-one',
                'bg-bg-one',
                '[&>[data-pc-name=pcfilteroperatordropdown]]:w-full',
            ],
        },
        filter: ({ instance }) => ({
            class: [
                {
                    'flex items-center w-full gap-2': instance.display === 'row',
                    'inline-flex ml-auto': instance.display === 'menu',
                },
            ],
        }),
        filterRuleList: 'flex flex-col gap-2',
        filterRule: 'flex flex-col gap-2',
        filterButtonbar: 'flex items-center justify-between p-0',
        filterAddButtonContainer: '[&>[data-pc-name=pcfilteraddrulebutton]]:w-full',
        rowToggleButton: {
            class: [
                'relative',

                // Flex & Alignment
                'inline-flex items-center justify-center',
                'text-left',

                // Spacing
                'm-0 p-0',

                // Size
                'w-8 h-8',

                // Shape
                'border-0 rounded-full',

                // Color
                'text-text-color',
                'bg-transparent',
                'focus-visible:outline-none focus-visible:outline-offset-0',
                'focus-visible:ring focus-visible:ring-primary-400/50',

                // Transition
                'transition duration-200',

                // Misc
                'overflow-hidden',
                'cursor-pointer select-none',
            ],
        },
        columnResizer: {
            class: [
                'block',

                // Position
                'absolute top-0 right-0',

                // Sizing
                'w-2 h-full',

                // Spacing
                'm-0 p-0',

                // Color
                'border border-transparent',

                // Misc
                'cursor-col-resize',
            ],
        },
        transition: {
            class: 'p-4 flex flex-col gap-2',
            enterFromClass: 'opacity-0 scale-y-[0.8]',
            enterActiveClass:
                'transition-[transform,opacity] duration-[120ms] ease-[cubic-bezier(0,0,0.2,1)]',
            leaveActiveClass: 'transition-opacity duration-100 ease-linear',
            leaveToClass: 'opacity-0',
        },
    },
    bodyRow: ({ context, props }) => ({
        class: [
            // Color
            { 'bg-surface-hover': context.selected },
            { 'bg-bg-one text-text-color': !context.selected },
            { 'font-bold bg-bg-one z-20': props.frozenRow },
            {
                'odd:bg-bg-one odd:text-text-color even:bg-surface-hover even:text-text-color':
                    context.stripedRows,
            },

            // State
            {
                'focus:outline-none focus:outline-offset-0 focus:ring focus:ring-primary-400/50 ring-inset':
                    context.selectable,
            },
            {
                'hover:bg-surface-300/20 hover:text-surface-600':
                    props.selectionMode && !context.selected,
            },

            // Transition
            {
                'transition duration-200':
                    (props.selectionMode && !context.selected) || props.rowHover,
            },

            // Misc
            { 'cursor-pointer': props.selectionMode },
        ],
    }),
    rowExpansion: {
        class: 'bg-surface-0 text-surface-600',
    },
    rowGroupHeader: {
        class: ['sticky z-20', 'bg-surface-0 text-surface-600'],
    },
    rowGroupFooter: {
        class: ['sticky z-20', 'bg-surface-0 text-surface-600'],
    },
    rowToggleButton: {
        class: [
            'relative',

            // Flex & Alignment
            'inline-flex items-center justify-center',
            'text-left',

            // Spacing
            'm-0 p-0',

            // Size
            'w-8 h-8',

            // Shape
            'border-0 rounded-full',

            // Color
            'text-surface-500',
            'bg-transparent',
            'focus-visible:outline-none focus-visible:outline-offset-0',
            'focus-visible:ring focus-visible:ring-primary-400/50',

            // Transition
            'transition duration-200',

            // Misc
            'overflow-hidden',
            'cursor-pointer select-none',
        ],
    },
    rowToggleIcon: {
        class: 'inline-block w-4 h-4',
    },
    columnResizeIndicator: {
        class: 'absolute hidden w-[2px] z-20 bg-primary',
    },
}
