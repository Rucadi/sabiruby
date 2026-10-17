# For each receiver of prelude.rb: every method it can call (public, protected, private), the
# owner the VM finds and the parameters it reports. One line per method:
#   receiver TAB name TAB owner TAB parameters.inspect
SW_RECV.each do |key, mk|
  r = mk.call
  names = (r.public_methods + r.protected_methods + r.private_methods).uniq.sort
  names.each do |m|
    um = (r.method(m) rescue nil)
    owner = um ? (um.owner.inspect rescue "?") : "?"
    params = um ? (um.parameters.inspect rescue "?") : "?"
    puts "#{key}\t#{m}\t#{sw_esc(owner)}\t#{params}"
  end
end
