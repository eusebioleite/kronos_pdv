create or replace trigger kronos.kronos_pdv after
	insert or update or delete on kronos.f_prgven
	for each row
declare
	v_status char(1);
begin
	select 
		pdv_status
	  into
		v_status
	  from f_pedvenda
	 where pdv_numped = NVL(:new.prv_numped, :old.prv_numped);

	if deleting then
		update inventario.cards
			set
				status = 'EXCLUIR'
		where pedido = :old.prv_numped;
		return;
	end if;

	if (inserting or updating) and v_status = 'A' then
		merge into inventario.cards cards
		using (select :new.prv_numped as pedido from dual) dados
		on (cards.pedido = dados.pedido)
		when matched then
			update set 
				cards.status = 'ATUALIZAR'
		when not matched then
			insert (pedido, status)
			values (dados.pedido, 'NOVO');
	end if;

exception
	when others then
		null;
end;