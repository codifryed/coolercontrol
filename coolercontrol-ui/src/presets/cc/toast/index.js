export default {
    root: ({ props }) => ({
        class: [
            //Size and Shape
            'w-96 rounded-lg',

            // Positioning
            {
                '-translate-x-2/4':
                    props.position === 'top-center' || props.position === 'bottom-center',
            },
        ],
    }),
    message: ({ props }) => ({
        class: [
            'my-4 rounded-lg w-full',
            'border-0 border-l-[6px]',
            'backdrop-blur-[10px] shadow-lg',

            // Colors
            {
                'bg-bg-two/50': props.message.severity === 'info' ||
                    props.message.severity === 'success' ||
                    props.message.severity === 'warn' ||
                    props.message.severity === 'error',
            },
            {
                'border-accent': props.message.severity === 'info',
                'border-green': props.message.severity === 'success',
                'border-yellow': props.message.severity === 'warn',
                'border-red': props.message.severity === 'error',
            },
            {
                'text-text-color': props.message.severity === 'info' ||
                    props.message.severity === 'success',
                'text-yellow': props.message.severity === 'warn',
                'text-red': props.message.severity === 'error',
            },
        ],
    }),
    messageContent: ({ props }) => ({
        class: [
            'flex p-4',
            {
                'items-start': props.message.summary,
                'items-center': !props.message.summary,
            },
        ],
    }),
    messageIcon: ({ props }) => ({
        class: [
            // Sizing and Spacing
            'w-6 h-6',
            'text-lg leading-none mr-2 shrink-0',

            // Colors
            {
                'text-accent': props.message.severity === 'info',
                'text-green': props.message.severity === 'success',
                'text-yellow': props.message.severity === 'warn',
                'text-red': props.message.severity === 'error',
            },
        ],
    }),
    messageText: {
        class: [
            // Font and Text
            'text-base leading-none',
            'ml-2',
            'flex-1',
        ],
    },
    summary: {
        class: 'font-bold block',
    },
    detail: ({ props }) => ({
        class: ['block', { 'mt-2': props.message.summary }],
    }),
    closeButton: {
        class: [
            // Flexbox
            'flex items-center justify-center',

            // Size
            'w-8 h-8',

            // Spacing and Misc
            'ml-auto  relative',

            // Shape
            'rounded-full',

            // Colors
            'bg-transparent outline-0',

            // Transitions
            'transition duration-200 ease-in-out',

            // States
            'hover:bg-surface-hover/50',

            // Misc
            'overflow-hidden',
        ],
    },
    transition: {
        enterFromClass: 'opacity-0 translate-y-2/4',
        enterActiveClass: 'transition-[transform,opacity] duration-300',
        leaveFromClass: 'max-h-[1000px]',
        leaveActiveClass:
            '!transition-[max-height_.45s_cubic-bezier(0,1,0,1),opacity_.3s,margin-bottom_.3s] overflow-hidden',
        leaveToClass: 'max-h-0 opacity-0 mb-0',
    },
}
