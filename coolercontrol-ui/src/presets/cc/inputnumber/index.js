export default {
    root: ({ props, parent }) => ({
        class: [
            // Flex
            'inline-flex',
            'relative',
            { 'flex-col': props.showButtons && props.buttonLayout == 'vertical' },
            { 'flex-1 w-[1%]': parent.instance.$name == 'InputGroup' },
            { 'w-full': props.fluid },

            // Shape
            { 'first:rounded-l-md rounded-none last:rounded-r-md': parent.instance.$name == 'InputGroup' && !props.showButtons },
            { 'border-0 border-y border-l last:border-r border-surface-300 dark:border-surface-600': parent.instance.$name == 'InputGroup' && !props.showButtons },
            { 'first:ml-0 -ml-px': parent.instance.$name == 'InputGroup' && !props.showButtons },

            //Sizing
            { '!w-16': props.showButtons && props.buttonLayout == 'vertical' }
        ]
    }),
    pcInput: {
        root: ({ parent, context }) => ({
            class: [
                // Font
                // 'text-base leading-none',
                // 'leading-[normal]',

                // Display
                'flex-auto',
                { 'w-[1%]': parent.props.fluid },

                //Text
                { 'text-center': parent.props.showButtons && parent.props.buttonLayout == 'vertical' },

                // Spacing
                'py-2 px-3',
                'm-0',

                // Shape
                'rounded-md',
                { 'rounded-l-none rounded-r-none': parent.props.showButtons && parent.props.buttonLayout == 'horizontal' },
                { 'rounded-none': parent.props.showButtons && parent.props.buttonLayout == 'vertical' },

                { 'border-0': parent.instance.$parentInstance?.$name == 'InputGroup' && !parent.props.showButtons },

                // Colors
                'text-surface-800 dark:text-white/80',
                'placeholder:text-surface-400 dark:placeholder:text-surface-500',
                { 'bg-surface-0 dark:bg-surface-900': !context.disabled },
                'border',
                { 'border-surface-300 dark:border-surface-700': !parent.props.invalid },

                // Invalid State
                'invalid:focus:ring-red-200',
                'invalid:hover:border-red-500',
                { 'border-red-500 dark:border-red-400': parent.props.invalid },

                // States
                { 'hover:border-primary': !parent.props.invalid },
                'focus:outline-none focus:outline-offset-0 focus:ring-1 focus:ring-primary-500/50 dark:focus:ring-primary-400/50 focus:z-10',
                { 'opacity-60 select-none pointer-events-none cursor-default': context.disabled },

                // Filled State *for FloatLabel
                { filled: parent.instance?.$parentInstance?.$name == 'FloatLabel' && parent.state.d_modelValue !== null },

                //Position
                { 'order-2': parent.props.buttonLayout == 'horizontal' || parent.props.buttonLayout == 'vertical' }
            ]
        })
    },
    buttonGroup: ({ props }) => ({
        class: [
            'absolute',

            // Flex
            'flex',
            'flex-col',

            'top-px right-px',

            { 'h-[calc(100%-2px)]': props.showButtons && props.buttonLayout === 'stacked' }
        ]
    }),
    incrementButton: ({ props }) => ({
        class: [
            // Display
            { 'flex flex-initial shrink-0': props.showButtons && props.buttonLayout === 'horizontal' },
            { 'flex flex-auto': props.showButtons && props.buttonLayout === 'stacked' },

            // Alignment
            'items-center',
            'justify-center',
            'text-center align-bottom',

            //Position
            'relative',
            { 'order-3': props.showButtons && props.buttonLayout === 'horizontal' },
            { 'order-1': props.showButtons && props.buttonLayout === 'vertical' },

            //Color
            'text-primary-contrast',
            'bg-primary',
            'border-primary',

            // Sizing
            'w-[3rem]',
            { 'px-4 py-3': props.showButtons && props.buttonLayout !== 'stacked' },
            { 'p-0': props.showButtons && props.buttonLayout === 'stacked' },
            { 'w-full': props.showButtons && props.buttonLayout === 'vertical' },

            // Shape
            'rounded-md',
            { 'rounded-tl-none rounded-br-none rounded-bl-none': props.showButtons && props.buttonLayout == 'stacked' },
            { 'rounded-bl-none rounded-tl-none': props.showButtons && props.buttonLayout == 'horizontal' },
            { 'rounded-bl-none rounded-br-none': props.showButtons && props.buttonLayout == 'vertical' },

            //States
            'focus:outline-none focus:outline-offset-0 focus:ring',
            'hover:bg-primary-emphasis hover:border-primary-emphasis',

            //Misc
            'cursor-pointer overflow-hidden select-none'
        ]
    }),
    incrementIcon: 'inline-block w-4 h-4',
    decrementButton: ({ props }) => ({
        class: [
            // Display
            { 'flex flex-initial shrink-0': props.showButtons && props.buttonLayout === 'horizontal' },
            { 'flex flex-auto': props.showButtons && props.buttonLayout === 'stacked' },

            // Alignment
            'items-center',
            'justify-center',
            'text-center align-bottom',

            //Position
            'relative',
            { 'order-1': props.showButtons && props.buttonLayout == 'horizontal' },
            { 'order-3': props.showButtons && props.buttonLayout == 'vertical' },

            //Color
            'text-primary-contrast',
            'bg-primary',
            'border-primary',

            // Sizing
            'w-[3rem]',
            { 'px-4 py-3': props.showButtons && props.buttonLayout !== 'stacked' },
            { 'p-0': props.showButtons && props.buttonLayout == 'stacked' },
            { 'w-full': props.showButtons && props.buttonLayout == 'vertical' },

            // Shape
            'rounded-md',
            { 'rounded-tr-none rounded-tl-none rounded-bl-none': props.showButtons && props.buttonLayout == 'stacked' },
            { 'rounded-tr-none rounded-br-none ': props.showButtons && props.buttonLayout == 'horizontal' },
            { 'rounded-tr-none rounded-tl-none ': props.showButtons && props.buttonLayout == 'vertical' },

            //States
            'focus:outline-none focus:outline-offset-0 focus:ring',
            'hover:bg-primary-emphasis hover:border-primary-emphasis',

            //Misc
            'cursor-pointer overflow-hidden select-none'
        ]
    }),
    decrementIcon: 'inline-block w-4 h-4'
};
