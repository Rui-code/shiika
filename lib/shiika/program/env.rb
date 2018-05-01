require 'shiika/type'

module Shiika
  class Program
    # Environment
    # Used both by Shiika::Program and Shiika::Evaluator
    class Env
      include Type

      # data: {
      #     sk_classes: {String => Program::SkClass},
      #     typarams: {String => Type::TyParam},
      #     local_vars: {String => Program::Lvar},
      #     sk_self: Program::SkClass,
      #   }
      def initialize(data)
        @data = {
          sk_classes: {},
          typarams: {},
          local_vars: {},
          constants: {},
          sk_self: :notset,
        }.merge(data)
      end

      # Create new instance of `Env` by merging `hash` into the one at the key
      def merge(key, x)
        if x.is_a?(Hash)
          self.class.new(@data.merge({key => @data[key].merge(x)}))
        else
          self.class.new(@data.merge({key => x}))
        end
      end

      def check_type_exists(type)
        raise "bug: #{type.inspect}" unless type.is_a?(TyRaw)
        if !@data[:sk_classes].key?(type.name) && type.name != "Void"
          raise ProgramError, "unknown type: #{name}"
        end
      end

      def find_class(name)
        return @data[:sk_classes].fetch(name)
      end

      def find_const(name)
        return @data[:constants].fetch(name)
      end

      def find_lvar(name)
        unless (lvar = @data[:local_vars][name])
          raise SkNameError, "undefined local variable: #{name}"
        end
        return lvar
      end

      # Find Program::SkIvar
      def find_ivar(name)
        unless (sk_self = @data[:sk_self])
          raise ProgramError, "ivar reference out of a class: #{name}" 
        end
        unless (ivar = sk_self.sk_ivars[name])
          raise SkNameError, "class #{sk_self.name} does not have "+
            "an instance variable #{name}"
        end
        return ivar
      end

      def find_method(receiver_type, name)
        raise receiver_type.inspect unless receiver_type.is_a?(TyRaw) || receiver_type.is_a?(TyMeta)
        sk_class = @data[:sk_classes].fetch(receiver_type.name)
        return sk_class.find_method(name)
      end
    end
  end
end
