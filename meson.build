project('mod-manager', 'vala', 'c')

vapi_dir = meson.current_source_dir() / 'vapi'
add_project_arguments(['--vapidir', vapi_dir], language: 'vala')

# blocked by GLib.SubprocessLauncher.spawnv()
# add_project_arguments(['--enable-experimental-non-null'], language: 'vala')

subdir('src')
subdir('dist')
# subdir('tests')
