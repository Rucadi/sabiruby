# Built only inside a throwaway container by tools/rc2_aspec.sh; never part of SabiRuby.
MRuby::Gem::Specification.new('mruby-aspec-dump') do |spec|
  spec.license = 'MIT'
  spec.author  = 'SabiRuby'
  spec.summary = 'Kernel#__aspec(mod, name): the argument spec mruby keeps for a method'
end
