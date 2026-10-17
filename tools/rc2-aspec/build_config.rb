# The reference's default build (build_config/default.rb: the default gembox), plus the dump gem.
MRuby::Build.new do |conf|
  conf.toolchain
  conf.gembox 'default'
  conf.gem '/aspec/gem'
end
