import { describe, it, expect } from 'vitest'
import {
  HookAction,
  SessionState,
  ContextInjectionRole,
  ApprovalDefault,
  UserMessageLevel,
  Role,
} from '../index.js'

describe('enum types', () => {
  describe('HookAction', () => {
    it('has all expected variants with correct string values', () => {
      expect(HookAction.Continue).toBe('Continue')
      expect(HookAction.Deny).toBe('Deny')
      expect(HookAction.Modify).toBe('Modify')
      expect(HookAction.InjectContext).toBe('InjectContext')
      expect(HookAction.AskUser).toBe('AskUser')
    })
  })

  describe('SessionState', () => {
    it('has all expected variants with correct string values', () => {
      expect(SessionState.Running).toBe('Running')
      expect(SessionState.Completed).toBe('Completed')
      expect(SessionState.Failed).toBe('Failed')
      expect(SessionState.Cancelled).toBe('Cancelled')
    })
  })

  describe('ContextInjectionRole', () => {
    it('has all expected variants with correct string values', () => {
      expect(ContextInjectionRole.System).toBe('System')
      expect(ContextInjectionRole.User).toBe('User')
      expect(ContextInjectionRole.Assistant).toBe('Assistant')
    })
  })

  describe('ApprovalDefault', () => {
    it('has all expected variants with correct string values', () => {
      expect(ApprovalDefault.Allow).toBe('Allow')
      expect(ApprovalDefault.Deny).toBe('Deny')
    })
  })

  describe('UserMessageLevel', () => {
    it('has all expected variants with correct string values', () => {
      expect(UserMessageLevel.Info).toBe('Info')
      expect(UserMessageLevel.Warning).toBe('Warning')
      expect(UserMessageLevel.Error).toBe('Error')
    })
  })

  describe('Role', () => {
    it('has all expected variants with correct string values', () => {
      expect(Role.System).toBe('System')
      expect(Role.User).toBe('User')
      expect(Role.Assistant).toBe('Assistant')
      expect(Role.Tool).toBe('Tool')
    })
  })
})
