;;; Gymnast public language surface.
;;;
;;; Leaf forms are fexprs: their operands are captured without evaluation.
;;; MODULE is a vau operative: it selectively evaluates allow-listed child
;;; declarations in the caller's lexical environment.

(def $gymnast-kernel-heads
  '(import application actor type component interface port state flow behavior
    invariant constraint synthesis acceptance))

(defun gymnast-trusted-surface-head-p (head)
  (or (member head $gymnast-kernel-heads)
    (getp head "gymnast.surface-macro")))

(defun gymnast-trusted-surface-form-p (form)
  (and (consp form)
    (symbolp (car form))
    (gymnast-trusted-surface-head-p (car form))))

(defun gymnast-container-parts (forms attributes children)
  (cond
    ((null forms) (list attributes children))
    ((gymnast-keyword-p (car forms))
      (if (null (cdr forms))
        (list attributes
          (append children
            (list (gymnast-make-invalid-surface
                (car forms)
                "attribute is missing its value"))))
        (gymnast-container-parts
          (cdr (cdr forms))
          (append attributes (list (car forms) (cadr forms)))
          children)))
    (t (gymnast-container-parts
        (cdr forms) attributes (append children (list (car forms)))))))

(defun gymnast-eval-surface-forms (forms env)
  (if (null forms)
    nil
    (let* ((form (car forms))
        (value
          (if (gymnast-trusted-surface-form-p form)
            (eval form env)
            (gymnast-make-invalid-surface
              form
              "module body admits only kernel forms and trusted surface macros"))))
      (cons value (gymnast-eval-surface-forms (cdr forms) env)))))

(defun gymnast-surface-leaf (kind operands mechanism)
  (if (null operands)
    (gymnast-make-invalid-surface kind "declaration requires a name")
    (gymnast-make-surface kind (car operands) (cdr operands) nil mechanism)))

(defexpr import (operands) (gymnast-surface-leaf 'import operands 'fexpr))
(defexpr application (operands) (gymnast-surface-leaf 'application operands 'fexpr))
(defexpr actor (operands) (gymnast-surface-leaf 'actor operands 'fexpr))
(defexpr type (operands) (gymnast-surface-leaf 'type operands 'fexpr))
(defexpr component (operands) (gymnast-surface-leaf 'component operands 'fexpr))
(defexpr interface (operands) (gymnast-surface-leaf 'interface operands 'fexpr))
(defexpr port (operands) (gymnast-surface-leaf 'port operands 'fexpr))
(defexpr state (operands) (gymnast-surface-leaf 'state operands 'fexpr))
(defexpr flow (operands) (gymnast-surface-leaf 'flow operands 'fexpr))
(defexpr behavior (operands) (gymnast-surface-leaf 'behavior operands 'fexpr))
(defexpr invariant (operands) (gymnast-surface-leaf 'invariant operands 'fexpr))
(defexpr constraint (operands) (gymnast-surface-leaf 'constraint operands 'fexpr))
(defexpr synthesis (operands) (gymnast-surface-leaf 'synthesis operands 'fexpr))
(defexpr acceptance (operands) (gymnast-surface-leaf 'acceptance operands 'fexpr))

(defvau module (operands env)
  "Capture one module. Only trusted declaration forms are evaluated."
  (if (null operands)
    (gymnast-make-invalid-surface 'module "module requires a name")
    (let* ((name (car operands))
        (parts (gymnast-container-parts (cdr operands) nil nil))
        (attributes (car parts))
        (child-forms (cadr parts))
        (children (gymnast-eval-surface-forms child-forms env)))
      (gymnast-make-surface 'module name attributes children 'vau))))

;;; DEFSPEC is transparent authoring sugar.  It introduces no new semantic
;;; form: expansion is a DEF whose value is an ordinary MODULE declaration.

(defmacro defspec (name &rest body)
  `(def ,name (module ,name ,@body)))

;;; A trusted example of macro-based convenience syntax.  It lowers a profile
;;; selection into the kernel IMPORT form and is inspectable with macroexpand.

(defmacro use-profile (name version &rest arguments)
  `(import ,name
    :version ,version
    :arguments ,arguments
    :authority authoritative))

(putp 'use-profile "gymnast.surface-macro" t)
