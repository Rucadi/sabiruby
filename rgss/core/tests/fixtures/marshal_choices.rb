# MRI-compatible event-command graph: independent copies retain internal aliases
# and cycles, as Interpreter#setup_choices requires before modifying commands.
module MarshalFixture
  class Command
    attr_accessor :code, :parameters
    def initialize(code, parameters)
      @code = code
      @parameters = parameters
    end
  end
end
text = "Elección"
command = MarshalFixture::Command.new(102, [[text, "No"], 2])
cycle = []
cycle << cycle
$marshal_graph = [command, command, cycle, text, text,
                  {-257 => 2**64, :flag => true},
                  [-123, -124, -256, -257, 0, 122, 123, 2.5, 1.0/0.0, -(2**70)],
                  Hash.new(7)]
copy = Marshal.restore(Marshal.dump($marshal_graph))
puts copy[0].class == MarshalFixture::Command
puts copy[0].equal?(copy[1])
puts copy[2].equal?(copy[2][0])
puts copy[3].equal?(copy[4])
puts copy[0].parameters[0][0].equal?(copy[3])
puts copy[5][-257] == 2**64
puts copy[6] == $marshal_graph[6]
puts copy[7][:missing] == 7
copy[0].parameters[0][0].replace("Changed")
puts $marshal_graph[0].parameters[0][0] == "Elección"
